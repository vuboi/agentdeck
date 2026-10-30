//! OpenCode through its headless server (`opencode serve`), kept running for the app. Each chat is one session on it: a
//! message is a `prompt` on that session, and the server's event stream (`/api/event`) is turned into the chat's events.
//! Text, tool calls, sub-agents, usage, permission prompts and forms are translated; a permission the mode or the approved
//! tools do not allow is refused and reported as a card at the turn's end (as Claude does).

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;

use ag_adapters::agent::interrupt::StopToken;
use ag_adapters::agent::opencode::{answer_permission, is_subagent, subagent_event, Answer, PermissionAnswer};
use ag_adapters::agent::permission::Mode;
use ag_adapters::agent::stall::{Watchdog, DEFAULT_STALL_LIMIT};
use ag_adapters::agent::{AgentEvent, PermissionRequest, TaskStatus, TurnRequest};
use futures_util::StreamExt;
use reqwest::Method;
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::sync::Mutex;

use crate::connectors::{command, which, KillTree};

/// How long the server keeps running with no turn.
const IDLE: Duration = Duration::from_secs(20 * 60);

/// How long an aborted turn is read for its end before it is given up.
const ABORT_GRACE: Duration = Duration::from_secs(5);

/// What a turn needs from the running server: its address, its password and the client that talks to it.
#[derive(Clone)]
struct Lease {
    base: String,
    password: String,
    http: reqwest::Client,
}

impl Lease {
    /// A call that answers at once: bounded, so a server that stopped answering cannot hang the turn.
    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.stream_request(method, path).timeout(CALL_TIMEOUT)
    }

    /// A call that is read for as long as the turn runs (the event stream): no limit on the whole body.
    fn stream_request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base))
            .basic_auth("opencode", Some(&self.password))
    }
}

/// How long a call that answers at once may take.
const CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// A turn that failed: its message, and whether the connection to the server is what broke.
struct Failure {
    message: String,
    connection: bool,
}

impl Failure {
    fn connection(error: impl ToString) -> Self {
        Self { message: error.to_string(), connection: true }
    }
}

impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self { message, connection: false }
    }
}

impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}

struct Server {
    lease: Lease,
    /// Turns using the server now.
    busy: usize,
    /// Changes at every turn start and end, so an idle timer stops the server only if nothing ran after it was set.
    generation: u64,
    // Before the child: dropped first, so the whole tree (the shell that starts `opencode`, then node) goes with it.
    _tree: KillTree,
    _child: tokio::process::Child,
}

static SERVER: LazyLock<Mutex<Option<Server>>> = LazyLock::new(|| Mutex::new(None));

/// Held while the server starts, so two turns do not start two, without the `SERVER` lock held for the seconds it takes.
static STARTING: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// A turn's hold on the server. Dropping it gives the server back, also when the task is aborted (a stopped run).
struct Held;

impl Drop for Held {
    fn drop(&mut self) {
        tauri::async_runtime::spawn(release());
    }
}

/// Starts `opencode serve` on a free local port, with a password only this app knows, and waits until it answers.
async fn start() -> Result<Server, String> {
    let binary = which("opencode").ok_or("OpenCode is not installed: install it in Settings > Connectors")?;
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map_err(|error| error.to_string())?
        .port();
    let password = uuid::Uuid::new_v4().to_string();
    let port_text = port.to_string();
    let child = command(&binary)
        .args(["serve", "--hostname", "127.0.0.1", "--port", port_text.as_str()])
        .env("OPENCODE_SERVER_PASSWORD", &password)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("could not start OpenCode: {error}"))?;
    if let Some(pid) = child.id() {
        crate::process_guard::adopt_pid(pid);
    }
    let server = Server {
        lease: Lease {
            base: format!("http://127.0.0.1:{port}"),
            password,
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .build()
                .map_err(|error| error.to_string())?,
        },
        busy: 0,
        generation: 0,
        _tree: KillTree::of(&child),
        _child: child,
    };
    for _ in 0..150 {
        let ready = server
            .lease
            .request(Method::GET, "/global/health")
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .is_ok_and(|response| response.status().is_success());
        if ready {
            return Ok(server);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    Err("OpenCode did not start in time".into())
}

/// The running server, started when there is none. The turn takes a lease on it until `release`.
async fn acquire() -> Result<(Lease, Held), String> {
    if let Some(lease) = take_lease().await {
        return Ok((lease, Held));
    }
    let _starting = STARTING.lock().await;
    // Another turn may have started it while this one waited.
    if let Some(lease) = take_lease().await {
        return Ok((lease, Held));
    }
    let server = start().await?;
    let mut slot = SERVER.lock().await;
    let server = slot.insert(server);
    server.busy += 1;
    server.generation += 1;
    Ok((server.lease.clone(), Held))
}

/// A lease on the running server, if there is one.
async fn take_lease() -> Option<Lease> {
    let mut slot = SERVER.lock().await;
    let server = slot.as_mut()?;
    server.busy += 1;
    server.generation += 1;
    Some(server.lease.clone())
}

/// A turn is done with the server: it stops once no turn has used it for `IDLE`.
async fn release() {
    let generation = {
        let mut slot = SERVER.lock().await;
        let Some(server) = slot.as_mut() else {
            return;
        };
        server.busy = server.busy.saturating_sub(1);
        server.generation += 1;
        server.generation
    };
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(IDLE).await;
        let mut slot = SERVER.lock().await;
        if slot
            .as_ref()
            .is_some_and(|server| server.busy == 0 && server.generation == generation)
        {
            *slot = None;
        }
    });
}

/// Drops the server after a failed turn, so the next turn starts a new one. Other chats may stream from it: it stays unless
/// this turn is the only one using it or the connection itself broke.
async fn evict(lease: &Lease, connection: bool) {
    let mut slot = SERVER.lock().await;
    if slot
        .as_ref()
        .is_some_and(|server| server.lease.base == lease.base && (connection || server.busy <= 1))
    {
        *slot = None;
    }
}

/// Runs one message as a turn of OpenCode and sends its events to the chat.
pub(crate) async fn run_turn(
    app: AppHandle,
    chat: String,
    turn: String,
    message_id: Option<String>,
    req: TurnRequest,
    mut stop: StopToken,
) {
    let mut out = |event: AgentEvent| crate::copilot::emit(&app, &chat, &turn, message_id.as_deref(), event);
    let (lease, _held) = match acquire().await {
        Ok(held) => held,
        Err(message) => return out(AgentEvent::Error { message }),
    };
    // `turn_on` has closed the turn with its terminal event already; an error back only asks for the server to be dropped.
    if let Err(failure) = turn_on(&lease, req, &mut stop, &mut out).await {
        evict(&lease, failure.connection).await;
    }
}

/// What a turn keeps while it runs: its session, its answer, the calls it made and the permissions it refused.
struct Turn {
    session: Option<String>,
    mode: Mode,
    /// The tools the user approved for this turn (`Bash(git status)`, `Write`).
    allowed: Vec<String>,
    /// The tool and input of each call id, as the server reported them.
    calls: HashMap<String, Call>,
    answer: Answer,
    /// Permission requests refused in this turn. They are reported before the turn ends, as Claude reports its denials.
    denied: Vec<PermissionRequest>,
}

impl Turn {
    fn new(mode: Mode, allowed: Vec<String>) -> Self {
        Self {
            session: None,
            mode,
            allowed,
            calls: HashMap::new(),
            answer: Answer::default(),
            denied: Vec::new(),
        }
    }
}

#[derive(Default)]
struct Call {
    name: String,
    input: Value,
}

/// How the server ended a turn that was followed to its end.
enum End {
    Finished,
    Failed(String),
}

/// The events of one turn, until the server ends its run, the turn is stopped or the connection breaks. Every turn ends
/// with exactly one terminal event (see `closing`); an `Err` only asks for the server to be dropped.
async fn turn_on(
    lease: &Lease,
    req: TurnRequest,
    stop: &mut StopToken,
    out: &mut impl FnMut(AgentEvent),
) -> Result<(), Failure> {
    let mut turn = Turn::new(Mode::parse(req.permission_mode.as_deref()), req.allowed_tools.clone());
    let result = follow(lease, &req, &mut *stop, &mut turn, &mut *out).await;
    let stopped = stop.is_stopped();
    if !turn.denied.is_empty() {
        out(AgentEvent::PermissionDenied {
            requests: std::mem::take(&mut turn.denied),
        });
    }
    let (message, outcome) = match result {
        Ok(End::Finished) => (None, Ok(())),
        Ok(End::Failed(message)) => (Some(message), Ok(())),
        Err(failure) => (Some(failure.message.clone()), Err(failure)),
    };
    out(closing(message.as_deref(), stopped, turn.session.clone(), &turn.answer));
    outcome
}

/// The event that closes a turn. A stopped turn ends as `Done` with the text it has, as a Claude or Codex turn does when its
/// stop is answered (`interrupt::stopped`): an interrupted run is not a failure.
fn closing(message: Option<&str>, stopped: bool, session: Option<String>, answer: &Answer) -> AgentEvent {
    match message {
        Some(message) if !stopped => AgentEvent::Error {
            message: message.to_owned(),
        },
        _ => AgentEvent::Done {
            session_id: session,
            text: answer.text(),
            cost_usd: answer.cost_usd(),
        },
    }
}

/// Creates or resumes the session, sends the message and follows the session's events until the run ends or is stopped.
async fn follow(
    lease: &Lease,
    req: &TurnRequest,
    stop: &mut StopToken,
    turn: &mut Turn,
    out: &mut impl FnMut(AgentEvent),
) -> Result<End, Failure> {
    let directory = req.cwd.clone().ok_or("OpenCode needs a working folder")?;
    let session_id = match req.session_id.clone().filter(|_| req.resume) {
        Some(id) => id,
        None => {
            let created = ok_json(
                lease
                    .request(Method::POST, "/api/session")
                    .json(&json!({ "location": { "directory": directory } }))
                    .send()
                    .await
                    .map_err(Failure::connection)?,
            )
            .await?;
            created["data"]["id"].as_str().ok_or("OpenCode did not return a session")?.to_owned()
        }
    };
    turn.session = Some(session_id.clone());
    out(AgentEvent::Session { session_id: session_id.clone() });
    if let Some((provider, id)) = req.model.as_deref().and_then(|model| model.split_once('/')) {
        ok_json(
            lease
                .request(Method::POST, &format!("/api/session/{session_id}/model"))
                .json(&json!({ "model": { "id": id, "providerID": provider } }))
                .send()
                .await
                .map_err(Failure::connection)?,
        )
        .await?;
    }

    // Subscribed before the prompt is sent, so no event of the turn is missed.
    let mut events = lease
        .stream_request(Method::GET, "/api/event")
        .send()
        .await
        .map_err(Failure::connection)?
        .bytes_stream();
    ok_json(
        lease
            .request(Method::POST, &format!("/api/session/{session_id}/prompt"))
            // OpenCode's default is steer: a prompt admitted while the session is still at a safe
            // boundary is delivered to the active run instead of being silently dropped.
            .json(&json!({ "text": req.prompt, "delivery": "steer", "resume": true }))
            .send()
            .await
            .map_err(Failure::connection)?,
    )
    .await?;

    let mut buffer: Vec<u8> = Vec::new();
    let mut aborting: Option<tokio::time::Instant> = None;
    // A turn that waits on a permission prompt nobody answers, or on a server that went silent, ends instead of hanging.
    let mut watchdog = Watchdog::new(DEFAULT_STALL_LIMIT);
    loop {
        let deadline = aborting.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(24 * 3600));
        tokio::select! {
            _ = tokio::time::sleep_until(watchdog.deadline()), if aborting.is_none() => {
                let _ = lease
                    .request(Method::POST, &format!("/api/session/{session_id}/interrupt"))
                    .send()
                    .await;
                return Err(Failure::from(watchdog.message()));
            }
            _ = stop.stopped(), if aborting.is_none() => {
                let _ = lease
                    .request(Method::POST, &format!("/api/session/{session_id}/interrupt"))
                    .send()
                    .await;
                aborting = Some(tokio::time::Instant::now() + ABORT_GRACE);
            }
            _ = tokio::time::sleep_until(deadline), if aborting.is_some() => return Ok(End::Finished),
            chunk = events.next() => match chunk {
                Some(Ok(bytes)) => {
                    buffer.extend_from_slice(&bytes);
                    while let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
                        let line: Vec<u8> = buffer.drain(..=end).collect();
                        let line = String::from_utf8_lossy(&line);
                        let Some(data) = line.trim().strip_prefix("data:") else {
                            continue;
                        };
                        let Ok(event) = serde_json::from_str::<Value>(data.trim()) else {
                            continue;
                        };
                        if event["data"]["sessionID"].as_str() == Some(session_id.as_str()) {
                            watchdog.touch();
                        }
                        if event["type"].as_str() == Some("permission.asked")
                            && event["data"]["sessionID"].as_str() == Some(session_id.as_str())
                        {
                            answer_request(lease, &session_id, &event["data"], turn).await?;
                            continue;
                        }
                        if event["type"].as_str() == Some("session.form")
                            && event["data"]["sessionID"].as_str() == Some(session_id.as_str())
                        {
                            let text = form_prompt(&event["data"]);
                            if !text.is_empty() {
                                out(AgentEvent::Delta { text });
                            }
                            // AgentDeck's question card answers on the next turn. Interrupt the
                            // server-side form wait so that turn can be admitted on this session.
                            let _ = lease
                                .request(Method::POST, &format!("/api/session/{session_id}/interrupt"))
                                .send()
                                .await;
                            return Ok(End::Finished);
                        }
                        let mut noted = |event: AgentEvent| {
                            watchdog.note(&event);
                            out(event);
                        };
                        match translate(&event, &session_id, &mut *turn, &mut noted) {
                            Step::Continue => {}
                            Step::Finished => return Ok(End::Finished),
                            Step::Failed(message) => return Ok(End::Failed(message)),
                        }
                    }
                }
                Some(Err(error)) => return Err(Failure::connection(error)),
                None => return Err(Failure::connection("OpenCode closed its event stream")),
            }
        }
    }
}

/// Answers a permission the server asks for in this turn: approved once when the mode or the approved tools allow it,
/// otherwise refused. A refusal is kept for the turn's end, where it is reported and the user can allow it.
async fn answer_request(lease: &Lease, session: &str, request: &Value, turn: &mut Turn) -> Result<(), Failure> {
    let id = request["id"].as_str().unwrap_or_default();
    let answer = answer_permission(request, turn.mode, &turn.allowed);
    let decision = match answer {
        PermissionAnswer::Approve => "once",
        PermissionAnswer::Refuse(_) => "reject",
    };
    ok_json(
        lease
            .request(Method::POST, &format!("/api/session/{session}/permission/{id}/reply"))
            .json(&json!({ "decision": decision }))
            .send()
            .await
            .map_err(Failure::connection)?,
    )
    .await?;
    if let PermissionAnswer::Refuse(refused) = answer {
        turn.denied.push(refused);
    }
    Ok(())
}

enum Step {
    Continue,
    Finished,
    Failed(String),
}

/// Turns one server event of this session into the chat's events.
fn translate(event: &Value, session: &str, turn: &mut Turn, out: &mut impl FnMut(AgentEvent)) -> Step {
    let data = &event["data"];
    if data["sessionID"].as_str() != Some(session) {
        return Step::Continue;
    }
    let id = data["id"].as_str().unwrap_or_default().to_owned();
    match event["type"].as_str().unwrap_or_default() {
        "session.text.delta" => {
            let text = data["delta"].as_str().unwrap_or_default();
            turn.answer.push_text(text);
            out(AgentEvent::Delta { text: text.to_owned() });
        }
        "session.tool.input.started" => {
            turn.calls.entry(id).or_default().name = data["name"].as_str().unwrap_or("tool").to_owned();
        }
        "session.tool.called" => {
            let call = turn.calls.entry(id.clone()).or_default();
            call.input = data["input"].clone();
            if call.name.is_empty() {
                call.name = "tool".into();
            }
            let name = call.name.clone();
            let background = is_subagent(&name);
            out(AgentEvent::ToolCall {
                id: id.clone(),
                name,
                input: data["input"].to_string(),
                change: None,
                background,
            });
            // A sub-agent's card starts with its call and ends with its result, as Claude's does.
            if background {
                out(subagent_event(&data["input"], &id, TaskStatus::Started, None));
            }
        }
        "session.tool.success" => {
            let call = turn.calls.remove(&id).unwrap_or_default();
            out(AgentEvent::ToolResult {
                id: id.clone(),
                output: tool_text(&data["content"]),
                is_error: false,
                image: None,
                change: None,
            });
            if is_subagent(&call.name) {
                out(subagent_event(&call.input, &id, TaskStatus::Done, None));
            }
        }
        "session.tool.failed" => {
            let call = turn.calls.remove(&id).unwrap_or_default();
            let message = data["error"]["message"].as_str().unwrap_or_default().to_owned();
            out(AgentEvent::ToolResult {
                id: id.clone(),
                output: message.clone(),
                is_error: true,
                image: None,
                change: None,
            });
            if is_subagent(&call.name) {
                out(subagent_event(&call.input, &id, TaskStatus::Failed, Some(message).filter(|text| !text.is_empty())));
            }
        }
        "session.step.ended" => {
            let tokens = &data["tokens"];
            turn.answer.end_step(data["cost"].as_f64().unwrap_or_default());
            out(AgentEvent::Usage {
                input_tokens: tokens["input"].as_u64().unwrap_or_default(),
                output_tokens: tokens["output"].as_u64().unwrap_or_default(),
                cache_read_tokens: tokens["cache"]["read"].as_u64().unwrap_or_default(),
                cache_write_tokens: tokens["cache"]["write"].as_u64().unwrap_or_default(),
                new_call: true,
            });
        }
        "session.execution.succeeded" => return Step::Finished,
        // Stopped by the user, or taken over by a newer message: the turn ends. A shutdown or an inactivity is a failure.
        "session.execution.interrupted" => {
            let reason = data["reason"].as_str().unwrap_or_default();
            if matches!(reason, "shutdown" | "inactivity") {
                return Step::Failed(format!("OpenCode ended the turn: {reason}"));
            }
            return Step::Finished;
        }
        "session.execution.failed" => {
            let message = data["error"]["message"].as_str().unwrap_or("OpenCode failed").to_owned();
            return Step::Failed(message);
        }
        _ => {}
    }
    Step::Continue
}

/// The text parts of a tool result, joined.
fn tool_text(content: &Value) -> String {
    content
        .as_array()
        .map(|parts| parts.iter().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join(""))
        .unwrap_or_default()
}

/// Converts OpenCode's native form event to the common question fence understood by the chat UI.
/// The next AgentDeck turn resumes the same OpenCode session with the selected answers.
fn form_prompt(form: &Value) -> String {
    let fields = form["fields"].as_array().or_else(|| form["data"]["fields"].as_array());
    let Some(fields) = fields else {
        return String::new();
    };
    let questions: Vec<Value> = fields
        .iter()
        .filter(|field| field["hidden"].as_bool() != Some(true))
        .filter_map(|field| {
            let question = field["title"].as_str().or_else(|| field["description"].as_str())?;
            let options = field["options"]
                .as_array()
                .map(|options| {
                    options
                        .iter()
                        .filter_map(|option| {
                            Some(json!({
                                "label": option["label"].as_str()?,
                                "description": option["description"].as_str().unwrap_or_default(),
                            }))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Some(json!({
                "id": field["key"].as_str().unwrap_or("answer"),
                "header": field["title"].as_str().unwrap_or("Answer"),
                "question": question,
                "multi": field["type"].as_str() == Some("multiselect"),
                "required": field["required"].as_bool().unwrap_or(false),
                "options": options,
            }))
        })
        .collect();
    if questions.is_empty() {
        return String::new();
    }
    format!(
        "\n\n```agentdeck-ask\n{}\n```\n",
        json!({ "questions": questions })
    )
}

/// The JSON body of a successful answer; an error answer becomes its message.
async fn ok_json(response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        Ok(body)
    } else {
        Err(body["error"]["message"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("OpenCode answered {status}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn() -> Turn {
        Turn::new(Mode::Ask, Vec::new())
    }

    /// Events as the server sent them (shapes checked against a live `opencode serve`).
    #[test]
    fn a_tool_turn_maps_to_chat_events() {
        let session = "ses_1";
        let lines = [
            r#"{"type":"session.text.delta","data":{"sessionID":"ses_1","assistantMessageID":"m","ordinal":0,"delta":"pong"}}"#,
            r#"{"type":"session.tool.input.started","data":{"sessionID":"ses_1","id":"call_1","name":"shell"}}"#,
            r#"{"type":"session.tool.called","data":{"sessionID":"ses_1","id":"call_1","input":{"command":"echo hi"},"executed":false}}"#,
            r#"{"type":"session.tool.success","data":{"sessionID":"ses_1","id":"call_1","content":[{"type":"text","text":"hi\n"}]}}"#,
            r#"{"type":"session.step.ended","data":{"sessionID":"ses_1","finish":"stop","tokens":{"input":10,"output":3,"reasoning":0,"cache":{"read":7,"write":1}},"cost":0}}"#,
            r#"{"type":"session.other","data":{"sessionID":"ses_2"}}"#,
            r#"{"type":"session.execution.succeeded","data":{"sessionID":"ses_1"}}"#,
        ];
        let mut turn = turn();
        let mut events = Vec::new();
        let mut out = |event: AgentEvent| events.push(event);
        let mut steps = Vec::new();
        for line in lines {
            let value: Value = serde_json::from_str(line).unwrap();
            steps.push(matches!(translate(&value, session, &mut turn, &mut out), Step::Finished));
        }
        assert_eq!(turn.answer.text(), "pong");
        assert_eq!(steps.last(), Some(&true));
        assert!(matches!(&events[0], AgentEvent::Delta { text } if text == "pong"));
        assert!(matches!(&events[1], AgentEvent::ToolCall { name, background: false, .. } if name == "shell"));
        assert!(matches!(&events[2], AgentEvent::ToolResult { output, is_error: false, .. } if output == "hi\n"));
        assert!(matches!(&events[3], AgentEvent::Usage { input_tokens: 10, output_tokens: 3, cache_read_tokens: 7, .. }));
        assert_eq!(events.len(), 4, "the other session's event is ignored");
    }

    /// A sub-agent call in the server's stream: its card starts with the call and ends with the result.
    #[test]
    fn a_subagent_call_is_a_task_card_from_its_call_to_its_result() {
        let session = "ses_1";
        let lines = [
            r#"{"type":"session.tool.input.started","data":{"sessionID":"ses_1","id":"call_s","name":"subagent"}}"#,
            r#"{"type":"session.tool.called","data":{"sessionID":"ses_1","id":"call_s","input":{"agent":"general","description":"Compute 2+2"},"executed":false}}"#,
            r#"{"type":"session.tool.success","data":{"sessionID":"ses_1","id":"call_s","content":[{"type":"text","text":"4"}]}}"#,
        ];
        let mut turn = turn();
        let mut events = Vec::new();
        let mut out = |event: AgentEvent| events.push(event);
        for line in lines {
            let value: Value = serde_json::from_str(line).unwrap();
            translate(&value, session, &mut turn, &mut out);
        }
        let task = |status| AgentEvent::Task {
            id: "call_s".into(),
            tool_use_id: Some("call_s".into()),
            status,
            label: Some("general: Compute 2+2".into()),
            summary: None,
            tools: None,
        };
        assert!(matches!(&events[0], AgentEvent::ToolCall { name, background: true, .. } if name == "subagent"));
        assert_eq!(events[1], task(TaskStatus::Started));
        assert!(matches!(&events[2], AgentEvent::ToolResult { is_error: false, .. }));
        assert_eq!(events[3], task(TaskStatus::Done));
        assert_eq!(events.len(), 4);
        assert!(turn.calls.is_empty(), "a finished call is forgotten");
    }

    #[test]
    fn a_failed_subagent_call_ends_its_task_as_failed() {
        let session = "ses_1";
        let mut turn = turn();
        let mut events = Vec::new();
        let mut out = |event: AgentEvent| events.push(event);
        let started = r#"{"type":"session.tool.input.started","data":{"sessionID":"ses_1","id":"call_f","name":"subagent"}}"#;
        let failed = r#"{"type":"session.tool.failed","data":{"sessionID":"ses_1","id":"call_f","error":{"message":"boom"}}}"#;
        for line in [started, failed] {
            let value: Value = serde_json::from_str(line).unwrap();
            translate(&value, session, &mut turn, &mut out);
        }
        assert!(matches!(
            events.last(),
            Some(AgentEvent::Task { status: TaskStatus::Failed, summary: Some(text), .. }) if text == "boom"
        ));
    }

    /// A stopped turn closes as Done with its text (as Claude's and Codex's do), and a failed one that was not stopped as
    /// an error.
    #[test]
    fn a_stopped_turn_closes_as_done_with_its_text_and_a_failed_one_as_an_error() {
        let mut answer = Answer::default();
        answer.push_text("partial");
        answer.end_step(0.5);
        let done = AgentEvent::Done {
            session_id: Some("ses_1".into()),
            text: "partial".into(),
            cost_usd: Some(0.5),
        };
        assert_eq!(closing(None, false, Some("ses_1".into()), &answer), done);
        assert_eq!(closing(Some("boom"), true, Some("ses_1".into()), &answer), done);
        assert_eq!(
            closing(Some("boom"), false, Some("ses_1".into()), &answer),
            AgentEvent::Error { message: "boom".into() }
        );
    }

    /// The user's stop ends the run (a finished turn); a server that shut down or went idle fails it.
    #[test]
    fn an_interrupted_run_ends_the_turn_and_a_shutdown_fails_it() {
        let mut turn = turn();
        let mut out = |_: AgentEvent| {};
        let interrupted = |reason: &str| {
            serde_json::from_str::<Value>(&format!(
                r#"{{"type":"session.execution.interrupted","data":{{"sessionID":"ses_1","reason":"{reason}"}}}}"#
            ))
            .unwrap()
        };
        assert!(matches!(translate(&interrupted("user"), "ses_1", &mut turn, &mut out), Step::Finished));
        assert!(matches!(translate(&interrupted("shutdown"), "ses_1", &mut turn, &mut out), Step::Failed(_)));
    }

    #[test]
    fn a_native_form_becomes_an_agentdeck_question_block() {
        let form = json!({
            "sessionID": "ses_1",
            "id": "frm_1",
            "title": "Choose a package manager",
            "fields": [{
                "key": "manager",
                "type": "string",
                "title": "Package manager",
                "required": true,
                "options": [
                    {"value": "pnpm", "label": "pnpm", "description": "Use pnpm"},
                    {"value": "npm", "label": "npm"}
                ]
            }]
        });
        let prompt = form_prompt(&form);
        assert!(prompt.contains("agentdeck-ask"));
        assert!(prompt.contains("\"question\":\"Package manager\""));
        assert!(prompt.contains("\"label\":\"pnpm\""));
        assert!(prompt.contains("\"required\":true"));
    }
}
