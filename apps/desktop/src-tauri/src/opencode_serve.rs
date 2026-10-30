//! OpenCode through its headless server (`opencode serve`), kept running for the app. Each chat is one session on it: a
//! message is a `prompt` on that session, and the server's event stream (`/api/event`) is turned into the chat's events.
//! Text, tool calls and usage are translated. Not yet: a message sent while a turn runs (steer), background work and
//! permission prompts.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;

use ag_adapters::agent::interrupt::StopToken;
use ag_adapters::agent::{AgentEvent, TurnRequest};
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
    fn request(&self, method: Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base))
            .basic_auth("opencode", Some(&self.password))
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
            http: reqwest::Client::new(),
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
async fn acquire() -> Result<Lease, String> {
    let mut slot = SERVER.lock().await;
    if slot.is_none() {
        *slot = Some(start().await?);
    }
    let server = slot.as_mut().ok_or("OpenCode did not start")?;
    server.busy += 1;
    server.generation += 1;
    Ok(server.lease.clone())
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

/// Drops the server after a broken connection, so the next turn starts a new one.
async fn evict() {
    *SERVER.lock().await = None;
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
    let lease = match acquire().await {
        Ok(lease) => lease,
        Err(message) => return out(AgentEvent::Error { message }),
    };
    let result = turn_on(&lease, req, &mut stop, &mut out).await;
    if let Err(message) = result {
        evict().await;
        out(AgentEvent::Error { message });
    }
    release().await;
}

/// The events of one turn, until its run ends, is aborted or the connection breaks.
async fn turn_on(
    lease: &Lease,
    req: TurnRequest,
    stop: &mut StopToken,
    out: &mut impl FnMut(AgentEvent),
) -> Result<(), String> {
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
                    .map_err(|error| error.to_string())?,
            )
            .await?;
            created["data"]["id"].as_str().ok_or("OpenCode did not return a session")?.to_owned()
        }
    };
    if let Some((provider, id)) = req.model.as_deref().and_then(|model| model.split_once('/')) {
        ok_json(
            lease
                .request(Method::POST, &format!("/api/session/{session_id}/model"))
                .json(&json!({ "model": { "id": id, "providerID": provider } }))
                .send()
                .await
                .map_err(|error| error.to_string())?,
        )
        .await?;
    }

    // Subscribed before the prompt is sent, so no event of the turn is missed.
    let mut events = lease
        .request(Method::GET, "/api/event")
        .send()
        .await
        .map_err(|error| error.to_string())?
        .bytes_stream();
    ok_json(
        lease
            .request(Method::POST, &format!("/api/session/{session_id}/prompt"))
            .json(&json!({ "text": req.prompt }))
            .send()
            .await
            .map_err(|error| error.to_string())?,
    )
    .await?;

    let mut names: HashMap<String, String> = HashMap::new();
    let mut answer = String::new();
    let mut buffer: Vec<u8> = Vec::new();
    let mut aborting: Option<tokio::time::Instant> = None;
    loop {
        let deadline = aborting.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(24 * 3600));
        tokio::select! {
            _ = stop.stopped(), if aborting.is_none() => {
                let _ = lease
                    .request(Method::POST, &format!("/api/session/{session_id}/interrupt"))
                    .send()
                    .await;
                aborting = Some(tokio::time::Instant::now() + ABORT_GRACE);
            }
            _ = tokio::time::sleep_until(deadline), if aborting.is_some() => return Ok(()),
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
                        match translate(&event, &session_id, &mut names, &mut answer, out) {
                            Step::Continue => {}
                            Step::Finished => {
                                if aborting.is_none() {
                                    out(AgentEvent::Done {
                                        session_id: Some(session_id.clone()),
                                        text: answer,
                                        cost_usd: None,
                                    });
                                }
                                return Ok(());
                            }
                            Step::Failed(message) => {
                                out(AgentEvent::Error { message });
                                return Ok(());
                            }
                        }
                    }
                }
                Some(Err(error)) => return Err(error.to_string()),
                None => return Err("OpenCode closed its event stream".into()),
            }
        }
    }
}

enum Step {
    Continue,
    Finished,
    Failed(String),
}

/// Turns one server event of this session into the chat's events.
fn translate(
    event: &Value,
    session: &str,
    names: &mut HashMap<String, String>,
    answer: &mut String,
    out: &mut impl FnMut(AgentEvent),
) -> Step {
    let data = &event["data"];
    if data["sessionID"].as_str() != Some(session) {
        return Step::Continue;
    }
    let id = data["id"].as_str().unwrap_or_default().to_owned();
    match event["type"].as_str().unwrap_or_default() {
        "session.text.delta" => {
            let text = data["delta"].as_str().unwrap_or_default();
            answer.push_str(text);
            out(AgentEvent::Delta { text: text.to_owned() });
        }
        "session.tool.input.started" => {
            names.insert(id, data["name"].as_str().unwrap_or("tool").to_owned());
        }
        "session.tool.called" => {
            let name = names.get(&id).cloned().unwrap_or_else(|| "tool".into());
            out(AgentEvent::ToolCall {
                id,
                name,
                input: data["input"].to_string(),
                change: None,
                background: false,
            });
        }
        "session.tool.success" => out(AgentEvent::ToolResult {
            id,
            output: tool_text(&data["content"]),
            is_error: false,
            image: None,
            change: None,
        }),
        "session.tool.failed" => out(AgentEvent::ToolResult {
            id,
            output: data["error"]["message"].as_str().unwrap_or_default().to_owned(),
            is_error: true,
            image: None,
            change: None,
        }),
        "session.step.ended" => {
            let tokens = &data["tokens"];
            out(AgentEvent::Usage {
                input_tokens: tokens["input"].as_u64().unwrap_or_default(),
                output_tokens: tokens["output"].as_u64().unwrap_or_default(),
                cache_read_tokens: tokens["cache"]["read"].as_u64().unwrap_or_default(),
                cache_write_tokens: tokens["cache"]["write"].as_u64().unwrap_or_default(),
                new_call: true,
            });
        }
        "session.execution.succeeded" => return Step::Finished,
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
        let mut names = HashMap::new();
        let mut answer = String::new();
        let mut events = Vec::new();
        let mut out = |event: AgentEvent| events.push(event);
        let mut steps = Vec::new();
        for line in lines {
            let value: Value = serde_json::from_str(line).unwrap();
            steps.push(matches!(translate(&value, session, &mut names, &mut answer, &mut out), Step::Finished));
        }
        assert_eq!(answer, "pong");
        assert_eq!(steps.last(), Some(&true));
        assert!(matches!(&events[0], AgentEvent::Delta { text } if text == "pong"));
        assert!(matches!(&events[1], AgentEvent::ToolCall { name, .. } if name == "shell"));
        assert!(matches!(&events[2], AgentEvent::ToolResult { output, is_error: false, .. } if output == "hi\n"));
        assert!(matches!(&events[3], AgentEvent::Usage { input_tokens: 10, output_tokens: 3, cache_read_tokens: 7, .. }));
        assert_eq!(events.len(), 4, "the other session's event is ignored");
    }
}
