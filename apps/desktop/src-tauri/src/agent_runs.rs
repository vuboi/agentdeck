//! The chat runs: one agent turn (a CLI, or a custom provider) per chat at a time, forwarded to the UI as
//! `agent-event`s, and stopped on request.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use ag_adapters::agent::antigravity::Antigravity;
use ag_adapters::agent::claude::Claude;
use ag_adapters::agent::codex::Codex;
use ag_adapters::agent::opencode::OpenCode;
use ag_adapters::agent::runner::{drive_turn, resolve_binary};
use ag_adapters::agent::interrupt::{StopHandle, StopToken, INTERRUPT_DEADLINE};
use ag_adapters::agent::warm::drive_warm_turn_with;
use ag_adapters::agent::{AgentEvent, Provider, TurnRequest};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::{custom_chat, mcp_manage};

/// One event of one chat, sent to the UI on the `agent-event` channel.
#[derive(Clone, Serialize)]
struct AgentEnvelope {
    session: String,
    /// Which run of the chat the event belongs to, so a stopped run's last events are never mistaken for the next one's.
    turn: String,
    event: AgentEvent,
}

/// The run in progress of one chat: which turn it is, so a stop meant for an older turn never ends a newer one.
struct Run {
    turn: String,
    handle: tauri::async_runtime::JoinHandle<()>,
    /// How to ask the run to stop gracefully (an interrupt, so its warm CLI survives). `None` for a run that cannot
    /// (a custom provider): it is aborted.
    stop: Option<StopHandle>,
}

/// Extra time after the adapter's own deadline before the task is aborted, for a run stuck where its deadline is not read
/// (waiting for the chat's lock, starting a CLI).
const ABORT_GRACE: Duration = Duration::from_secs(5);

/// Stops a run: an interrupt the CLI answers by ending the turn, so the process stays warm and the half-done step is kept.
/// The task is aborted (which kills the CLI's process tree) only if it is still running after `INTERRUPT_DEADLINE`.
fn stop_run(run: Run) {
    let Some(stop) = run.stop else {
        run.handle.abort();
        return;
    };
    stop.stop();
    let mut handle = run.handle;
    tauri::async_runtime::spawn(async move {
        if tokio::time::timeout(INTERRUPT_DEADLINE + ABORT_GRACE, &mut handle).await.is_err() {
            handle.abort();
        }
    });
}

/// The runs in progress, per chat, so one can be stopped.
static RUNNING: LazyLock<Mutex<HashMap<String, Run>>> = LazyLock::new(Default::default);

/// Keeps `handle` as the chat's run. A chat runs one turn at a time: the run it replaces is stopped.
fn track(session: String, turn: String, handle: tauri::async_runtime::JoinHandle<()>, stop: Option<StopHandle>) {
    if let Ok(mut running) = RUNNING.lock() {
        if let Some(previous) = running.insert(session, Run { turn, handle, stop }) {
            stop_run(previous);
        }
    }
}

/// A run that ended on its own leaves the table, unless a newer turn of the chat already took its place.
fn forget(session: &str, turn: &str) {
    if let Ok(mut running) = RUNNING.lock() {
        if running.get(session).is_some_and(|run| run.turn == turn) {
            running.remove(session);
        }
    }
}

fn emit(app: &AppHandle, session: &str, turn: &str, event: AgentEvent) {
    let _ = app.emit(
        "agent-event",
        AgentEnvelope {
            session: session.to_owned(),
            turn: turn.to_owned(),
            event,
        },
    );
}

fn provider_for(agent: &str) -> Result<Box<dyn Provider>, String> {
    match agent {
        "claude" => Ok(Box::new(Claude)),
        "codex" => Ok(Box::new(Codex::default())),
        "antigravity" => Ok(Box::new(Antigravity::default())),
        "opencode" => Ok(Box::new(OpenCode::default())),
        other => Err(format!("no agent provider for `{other}` yet")),
    }
}

/// Runs one turn for the app: resolves the binary, then forwards every event to the UI.
async fn run_turn(
    app: AppHandle,
    session: String,
    turn: String,
    provider: Box<dyn Provider>,
    req: TurnRequest,
    keep_open: bool,
    mut stop: StopToken,
) {
    if provider.id() == "antigravity" {
        mcp_manage::ensure_agy_link().await;
    }
    // What a kept CLI says on its own after a turn (a background job finished) reaches the chat through this.
    let tail_app = app.clone();
    ag_adapters::agent::tail::set_sink(std::sync::Arc::new(move |chat, turn, event| emit(&tail_app, chat, turn, event)));
    let binary = resolve_binary(provider.as_ref());
    // Claude, Codex and Antigravity keep their CLI open between the chat's turns (see `warm.rs`); OpenCode starts one per turn.
    // A turn that runs for another one (a side answer, a delegated worker, a step of a flow) is run once, not kept: it
    // would sit idle for twenty minutes and push the processes of the chats people are in out of the pool.
    let warm = keep_open
        && drive_warm_turn_with(binary.clone(), provider.as_ref(), &req, stop.clone(), |event| {
            emit(&app, &session, &turn, event)
        })
        .await;
    if !warm {
        // A per-turn process has no interrupt message: a stop drops the turn, which ends its process tree.
        tokio::select! {
            () = drive_turn(binary, provider.as_ref(), &req, |event| emit(&app, &session, &turn, event)) => {}
            () = stop.stopped() => {}
        }
    }
    forget(&session, &turn);
}

/// Ends the CLIs kept open between turns, for one agent or all (`agent: None`). Called after an account switch: a CLI
/// that is already running keeps the account it started with.
#[tauri::command]
pub(crate) fn agent_warm_reset(agent: Option<String>) {
    ag_adapters::agent::warm::reset(agent.as_deref());
}

/// Starts a chat's agent CLI while its message is still being typed (Claude, Codex, Antigravity keep it open between
/// turns), so the first turn does not wait for the CLI to load. The arguments are the ones `agent_send` will get.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // one parameter per field the frontend sends
pub(crate) async fn agent_prewarm(
    session: String,
    agent: String,
    agent_session: Option<String>,
    resume: bool,
    model: Option<String>,
    permission_mode: Option<String>,
    effort: Option<String>,
    allowed_tools: Vec<String>,
    cwd: Option<String>,
    api_key: Option<String>,
) -> Result<(), String> {
    if agent == "copilot" {
        // The SDK starts a scoped Copilot runtime when the user sends the first turn.
        return Ok(());
    }
    let provider = provider_for(&agent)?;
    let req = TurnRequest {
        prompt: String::new(),
        session_id: agent_session,
        resume,
        model,
        permission_mode,
        effort,
        allowed_tools,
        cwd: cwd.map(Into::into),
        owner: Some(session),
        api_key,
    };
    tauri::async_runtime::spawn(async move {
        if provider.id() == "antigravity" {
            mcp_manage::ensure_agy_link().await;
        }
        let binary = resolve_binary(provider.as_ref());
        ag_adapters::agent::warm::prewarm(binary, provider.as_ref(), &req).await;
    });
    Ok(())
}

/// Sends one user message to an agent CLI. Returns at once; the answer arrives as `agent-event`s.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // one parameter per field the frontend sends
pub(crate) async fn agent_send(
    app: AppHandle,
    session: String,
    turn: String,
    agent: String,
    prompt: String,
    agent_session: Option<String>,
    resume: bool,
    model: Option<String>,
    permission_mode: Option<String>,
    effort: Option<String>,
    allowed_tools: Vec<String>,
    cwd: Option<String>,
    api_key: Option<String>,
    keep_open: Option<bool>,
) -> Result<(), String> {
    if agent == "copilot" {
        let req = TurnRequest {
            prompt,
            session_id: agent_session,
            resume,
            model,
            permission_mode,
            effort,
            allowed_tools,
            cwd: cwd.map(Into::into),
            owner: Some(session.clone()),
            api_key,
        };
        let (stop, token) = StopHandle::new();
        let handle = tauri::async_runtime::spawn(crate::copilot::run_turn(
            app.clone(), session.clone(), turn.clone(), req, token,
        ));
        track(session, turn, handle, Some(stop));
        return Ok(());
    }
    let provider = provider_for(&agent)?;
    let req = TurnRequest {
        prompt,
        session_id: agent_session,
        resume,
        model,
        permission_mode,
        effort,
        allowed_tools,
        cwd: cwd.map(Into::into),
        owner: Some(session.clone()),
        api_key,
    };
    let (stop, token) = StopHandle::new();
    let handle = tauri::async_runtime::spawn(run_turn(
        app,
        session.clone(),
        turn.clone(),
        provider,
        req,
        keep_open.unwrap_or(true),
        token,
    ));
    track(session, turn, handle, Some(stop));
    Ok(())
}

/// Sends a message to a custom provider (an OpenAI-compatible server). Same contract as `agent_send`: returns
/// at once and the answer arrives as `agent-event`s. The whole conversation goes along: there is no CLI session.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // one parameter per field the frontend sends
pub(crate) async fn custom_send(
    app: AppHandle,
    session: String,
    turn: String,
    kind: String,
    base_url: String,
    api_key: Option<String>,
    model: String,
    messages: Vec<custom_chat::ChatMessage>,
    permission_mode: Option<String>,
    allowed_tools: Vec<String>,
    cwd: Option<String>,
    effort: Option<String>,
    restricted: Option<bool>,
) -> Result<(), String> {
    let config = custom_chat::Turn {
        kind,
        base_url,
        api_key,
        model,
        cwd: cwd.map(Into::into),
        permission_mode,
        allowed: allowed_tools,
        chat: session.clone(),
        effort,
        restricted: restricted.unwrap_or(false),
    };
    let (chat, this_turn) = (session.clone(), turn.clone());
    let handle = tauri::async_runtime::spawn(async move {
        custom_chat::run_turn(&app.clone(), config, messages, |event| {
            emit(&app, &chat, &this_turn, event)
        })
        .await;
        forget(&chat, &this_turn);
    });
    track(session, turn, handle, None);
    Ok(())
}

/// Hands a message to the turn that is running in a chat (Claude only). True when the turn took it: the CLI reads it at
/// its next step, in the same turn. False when the chat has no such turn; the caller then keeps the message for later.
#[tauri::command]
pub(crate) fn agent_inject(session: String, text: String) -> bool {
    ag_adapters::agent::warm::inject(&session, &text)
}

/// Stops the run of a chat: its CLI is interrupted and keeps running, warm (see `stop_run`). With `turn`, only that turn is stopped: a stop
/// that reaches this side after the chat's next message was sent must not kill the new run (it would never answer).
#[tauri::command]
pub(crate) fn agent_stop(session: String, turn: Option<String>) {
    // An answer the CLI began on its own (after background work) has no run of its own: it is interrupted here.
    if turn.as_deref().is_none_or(|turn| turn.starts_with("auto-")) {
        ag_adapters::agent::tail::interrupt(&session);
    }
    if let Ok(mut running) = RUNNING.lock() {
        let current = running.get(&session).map(|run| run.turn.as_str());
        if current.is_some() && (turn.is_none() || turn.as_deref() == current) {
            if let Some(run) = running.remove(&session) {
                stop_run(run);
            }
        }
    }
}
