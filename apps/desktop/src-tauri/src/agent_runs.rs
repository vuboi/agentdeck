//! The chat runs: one agent turn (a CLI, or a custom provider) per chat at a time, forwarded to the UI as
//! `agent-event`s, and stopped on request.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use ag_adapters::agent::antigravity::Antigravity;
use ag_adapters::agent::claude::Claude;
use ag_adapters::agent::codex::Codex;
use ag_adapters::agent::opencode::OpenCode;
use ag_adapters::agent::runner::{drive_turn, resolve_binary};
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
}

/// The runs in progress, per chat, so one can be stopped.
static RUNNING: LazyLock<Mutex<HashMap<String, Run>>> = LazyLock::new(Default::default);

/// Keeps `handle` as the chat's run. A chat runs one turn at a time: the run it replaces is stopped.
fn track(session: String, turn: String, handle: tauri::async_runtime::JoinHandle<()>) {
    if let Ok(mut running) = RUNNING.lock() {
        if let Some(previous) = running.insert(session, Run { turn, handle }) {
            previous.handle.abort();
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
) {
    if provider.id() == "antigravity" {
        mcp_manage::ensure_agy_link().await;
    }
    let binary = resolve_binary(provider.as_ref());
    drive_turn(binary, provider.as_ref(), &req, |event| {
        emit(&app, &session, &turn, event)
    })
    .await;
    forget(&session, &turn);
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
) -> Result<(), String> {
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
    };
    let handle = tauri::async_runtime::spawn(run_turn(
        app,
        session.clone(),
        turn.clone(),
        provider,
        req,
    ));
    track(session, turn, handle);
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
    };
    let (chat, this_turn) = (session.clone(), turn.clone());
    let handle = tauri::async_runtime::spawn(async move {
        custom_chat::run_turn(&app.clone(), config, messages, |event| {
            emit(&app, &chat, &this_turn, event)
        })
        .await;
        forget(&chat, &this_turn);
    });
    track(session, turn, handle);
    Ok(())
}

/// Stops the run of a chat: its CLI process is killed with the task. With `turn`, only that turn is stopped: a stop
/// that reaches this side after the chat's next message was sent must not kill the new run (it would never answer).
#[tauri::command]
pub(crate) fn agent_stop(session: String, turn: Option<String>) {
    if let Ok(mut running) = RUNNING.lock() {
        let current = running.get(&session).map(|run| run.turn.as_str());
        if current.is_some() && (turn.is_none() || turn.as_deref() == current) {
            if let Some(run) = running.remove(&session) {
                run.handle.abort();
            }
        }
    }
}
