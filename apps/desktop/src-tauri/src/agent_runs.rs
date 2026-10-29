//! The chat runs: one agent turn (a CLI, or a custom provider) per chat at a time, forwarded to the UI as
//! `agent-event`s, and stopped on request.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use ag_adapters::agent::antigravity::Antigravity;
use ag_adapters::agent::claude::Claude;
use ag_adapters::agent::codex::Codex;
use ag_adapters::agent::opencode::OpenCode;
use ag_adapters::agent::runner::{drive_turn, resolve_binary};
use ag_adapters::agent::interrupt::{StopHandle, StopToken, INTERRUPT_DEADLINE};
use ag_adapters::agent::warm::drive_warm_turn_with;
use ag_adapters::agent::{AgentEvent, ChatResponseCapabilities, Provider, TurnRequest};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::{custom_chat, mcp_manage};

/// A chat in "This computer" has no project path; give its CLI an intentional user-home cwd instead of letting it
/// inherit the directory the desktop app happened to be launched from (often the AgentDeck source repository).
fn home_directory() -> Option<PathBuf> {
    ["USERPROFILE", "HOME"]
        .into_iter()
        .find_map(|key| std::env::var_os(key).map(PathBuf::from).filter(|path| path.is_dir()))
}

fn working_directory(cwd: Option<String>) -> Option<PathBuf> {
    cwd.map(PathBuf::from).or_else(home_directory)
}

/// One event of one chat, sent to the UI on the `agent-event` channel.
#[derive(Clone, Serialize)]
struct AgentEnvelope {
    session: String,
    /// Which run of the chat the event belongs to, so a stopped run's last events are never mistaken for the next one's.
    turn: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_id: Option<String>,
    event: AgentEvent,
}

/// The run in progress of one chat: which turn it is, so a stop meant for an older turn never ends a newer one.
struct Run {
    handle: tauri::async_runtime::JoinHandle<()>,
    response: ChatResponseCapabilities,
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

/// The active runs, per visible chat and turn. A foreground listener and a background waiter can coexist in one chat.
static RUNNING: LazyLock<Mutex<HashMap<(String, String), Run>>> = LazyLock::new(Default::default);

/// Keeps one independently addressable run. A new turn never replaces another lane of the same visible chat.
fn track(
    session: String,
    turn: String,
    handle: tauri::async_runtime::JoinHandle<()>,
    stop: Option<StopHandle>,
    response: ChatResponseCapabilities,
) {
    if let Ok(mut running) = RUNNING.lock() {
        let key = (session, turn);
        if let Some(previous) = running.insert(key, Run { handle, stop, response }) {
            stop_run(previous);
        }
    }
}

/// A run that ended on its own leaves the table, unless a newer turn of the chat already took its place.
fn forget(session: &str, turn: &str) {
    if let Ok(mut running) = RUNNING.lock() {
        running.remove(&(session.to_owned(), turn.to_owned()));
    }
}

fn emit(app: &AppHandle, session: &str, turn: &str, message_id: Option<&str>, event: AgentEvent) {
    crate::flow_log::tap(session, turn, &event);
    let _ = app.emit(
        "agent-event",
        AgentEnvelope {
            session: session.to_owned(),
            turn: turn.to_owned(),
            message_id: message_id.map(str::to_owned),
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
#[allow(clippy::too_many_arguments)] // the turn identity, reply identity, provider, request and stop token are separate inputs
async fn run_turn(
    app: AppHandle,
    session: String,
    turn: String,
    provider: Box<dyn Provider>,
    req: TurnRequest,
    message_id: Option<String>,
    keep_open: bool,
    mut stop: StopToken,
) {
    let mut noted = crate::flow_log::TurnGuard::new(&session, &turn, provider.id(), req.model.as_deref().unwrap_or("-"));
    if provider.id() == "antigravity" {
        mcp_manage::ensure_agy_link().await;
    }
    // What a kept CLI says on its own after a turn (a background job finished) reaches the chat through this.
    let tail_app = app.clone();
    ag_adapters::agent::tail::set_sink(std::sync::Arc::new(move |chat, turn, event| emit(&tail_app, chat, turn, None, event)));
    let binary = resolve_binary(provider.as_ref());
    // Claude, Codex and Antigravity keep their CLI open between the chat's turns (see `warm.rs`); OpenCode starts one per turn.
    // A turn that runs for another one (a side answer, a delegated worker, a step of a flow) is run once, not kept: it
    // would sit idle for twenty minutes and push the processes of the chats people are in out of the pool.
    let warm = keep_open
        && drive_warm_turn_with(binary.clone(), provider.as_ref(), &req, stop.clone(), |event| {
            emit(&app, &session, &turn, message_id.as_deref(), event)
        })
        .await;
    if !warm {
        // A per-turn process has no interrupt message: a stop drops the turn, which ends its process tree.
        tokio::select! {
            () = drive_turn(binary, provider.as_ref(), &req, |event| emit(&app, &session, &turn, message_id.as_deref(), event)) => {}
            () = stop.stopped() => {}
        }
    }
    noted.finish(if stop.is_stopped() { "stopped" } else { "finished" });
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
        cwd: working_directory(cwd),
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
    message_id: Option<String>,
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
        let response = ChatResponseCapabilities::copilot();
        let req = TurnRequest {
            prompt,
            session_id: agent_session,
            resume,
            model,
            permission_mode,
            effort,
            allowed_tools,
            cwd: working_directory(cwd),
            owner: Some(session.clone()),
            api_key,
        };
        let (stop, token) = StopHandle::new();
        let mut noted = crate::flow_log::TurnGuard::new(&session, &turn, "copilot", req.model.as_deref().unwrap_or("-"));
        let run = crate::copilot::run_turn(app.clone(), session.clone(), turn.clone(), message_id.clone(), req, token);
        let handle = tauri::async_runtime::spawn(async move {
            run.await;
            noted.finish("finished");
        });
        track(session, turn, handle, Some(stop), response);
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
        cwd: working_directory(cwd),
        owner: Some(session.clone()),
        api_key,
    };
    let response = provider.chat_capabilities(&req);
    let (stop, token) = StopHandle::new();
    let handle = tauri::async_runtime::spawn(run_turn(
        app,
        session.clone(),
        turn.clone(),
        provider,
        req,
        message_id,
        keep_open.unwrap_or(true),
        token,
    ));
    track(session, turn, handle, Some(stop), response);
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
    message_id: Option<String>,
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
    let restricted = restricted.unwrap_or(false);
    let config = custom_chat::Turn {
        kind,
        base_url,
        api_key,
        model,
        cwd: if restricted { None } else { working_directory(cwd) },
        permission_mode,
        allowed: allowed_tools,
        chat: session.clone(),
        effort,
        restricted,
    };
    let response = ChatResponseCapabilities::custom();
    let (chat, this_turn) = (session.clone(), turn.clone());
    let mut noted = crate::flow_log::TurnGuard::new(&chat, &this_turn, "custom", &config.model);
    let handle = tauri::async_runtime::spawn(async move {
        custom_chat::run_turn(&app.clone(), config, messages, |event| {
            emit(&app, &chat, &this_turn, message_id.as_deref(), event)
        })
        .await;
        noted.finish("finished");
        forget(&chat, &this_turn);
    });
    track(session, turn, handle, None, response);
    Ok(())
}

/// Hands a message to a running turn only when its provider guarantees a distinct reply for that message. Merged steer
/// and native queue events stay under AgentDeck's message queue because this command has no independent reply identity.
#[tauri::command]
pub(crate) fn agent_inject(session: String, text: String) -> bool {
    let can_inject = RUNNING.lock().ok().is_some_and(|running| {
        let mut session_runs = running.iter().filter(|((chat, _), _)| chat == &session);
        match (session_runs.next(), session_runs.next()) {
            (Some((_, run)), None) => run.response.can_inject_as_distinct_reply(),
            _ => false,
        }
    });
    can_inject && ag_adapters::agent::warm::inject(&session, &text)
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
        let keys = running
            .keys()
            .filter(|(chat, run_turn)| chat == &session && turn.as_deref().is_none_or(|target| target == run_turn))
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            if let Some(run) = running.remove(&key) {
                stop_run(run);
            }
        }
    }
}

/// Stops every active agent turn and warm CLI when the desktop WebView reloads.
#[tauri::command]
pub(crate) fn agent_stop_all() {
    let runs = RUNNING
        .lock()
        .map(|mut running| std::mem::take(&mut *running))
        .unwrap_or_default();
    for ((session, turn), run) in runs {
        if turn.starts_with("auto-") {
            ag_adapters::agent::tail::interrupt(&session);
        }
        stop_run(run);
    }
    ag_adapters::agent::warm::reset(None);
}
