//! Codex kept open between the turns of a chat, through `codex app-server`.
//!
//! `codex exec` loads the user's config, MCP servers and plugins on every turn. The app server does it once and then
//! takes one `turn/start` per message over JSON-RPC on stdin/stdout. Its notifications are translated here into the
//! lines `codex exec --json` prints, so `Codex::parse_line` (commands, file diffs, MCP calls, usage) stays the only
//! place that knows what a step looks like. Answer and reasoning text are the exception: the server streams them, so
//! they go to the chat as they come instead of in one piece at the end. The process is pooled like the CLIs in
//! `warm.rs`, and can be started before the first message (`prewarm`).

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::time::Duration;

use serde_json::{json, Value};

use super::codex::APP_CONTEXT;
use super::interrupt::{self, Interrupter, Signal, StopToken};
use super::stall::Watchdog;
use super::warm::{holds, lock_of, park, send, settings_key, start, take_server, Warm, EXIT_GRACE};
use super::{AgentEvent, Provider, TurnRequest};

/// A reply to a handshake request that takes longer than this means the server is not coming up.
const HANDSHAKE: Duration = Duration::from_secs(60);

/// The thread's settings, as `thread/start` takes them. The model and the effort are not here: each turn names them
/// (`turn/start`), so changing either keeps the server and the conversation.
fn thread_params(req: &TurnRequest) -> Value {
    let (sandbox, approval) = match req.permission_mode.as_deref() {
        Some("acceptEdits") => ("workspace-write", "never"),
        // Approvals go to Codex's own reviewer, as `codex exec --approve-for-me` does.
        Some("auto") => ("workspace-write", "on-request"),
        Some("bypassPermissions") => ("danger-full-access", "never"),
        _ => ("read-only", "never"),
    };
    let mut config = json!({});
    if let Ok(bridge) = std::env::var("AGENTDECK_BRIDGE") {
        config["mcp_servers"] = json!({ "agentdeck": {
            "url": super::mcp_url(&bridge, req.owner.as_deref(), req.cwd.as_deref()),
            // Codex gives an MCP tool 60 s by default; delegate_task waits for another agent for many minutes.
            "tool_timeout_sec": 7500,
        }});
    }
    let mut params = json!({
        "approvalPolicy": approval,
        "sandbox": sandbox,
        "config": config,
        "developerInstructions": format!("{APP_CONTEXT}{}", super::claude::ASK_CONTEXT),
    });
    if req.permission_mode.as_deref() == Some("auto") {
        params["approvalsReviewer"] = json!("auto_review");
    }
    if let Some(dir) = req.cwd.as_deref().filter(|dir| dir.is_dir()) {
        params["cwd"] = json!(dir.to_string_lossy());
    }
    params
}

/// Turns the server's notifications into chat events, for one turn.
#[derive(Default)]
struct Translate {
    /// What each running command has printed so far (the exec format repeats the whole output).
    output: HashMap<String, String>,
    /// Answer and reasoning items already streamed: their finished copy is not sent again.
    streamed: HashSet<String>,
    /// The last thing shown was answer text: a new answer starts a new paragraph.
    after_text: bool,
    /// Token counts of the turn: the sum of its model calls.
    input: u64,
    cached: u64,
    written: u64,
    out: u64,
}

fn status(item: &Value) -> &'static str {
    match item["status"].as_str() {
        Some("inProgress") => "in_progress",
        Some("failed") | Some("declined") => "failed",
        _ => "completed",
    }
}

/// An app-server item in the shape `codex exec --json` gives it, or `None` for a kind the chat does not draw.
fn exec_item(item: &Value) -> Option<Value> {
    let id = &item["id"];
    Some(match item["type"].as_str()? {
        "commandExecution" => json!({
            "id": id, "type": "command_execution", "command": item["command"],
            "aggregated_output": item["aggregatedOutput"].as_str().unwrap_or_default(),
            "exit_code": item["exitCode"], "status": status(item),
        }),
        "agentMessage" => json!({ "id": id, "type": "agent_message", "text": item["text"] }),
        "reasoning" => {
            let parts = |key: &str| -> Vec<&str> { item[key].as_array().into_iter().flatten().filter_map(Value::as_str).collect() };
            let text = [parts("summary"), parts("content")].into_iter().find(|list| !list.is_empty()).unwrap_or_default().join("\n\n");
            json!({ "id": id, "type": "reasoning", "text": text })
        }
        "fileChange" => {
            let changes: Vec<Value> = item["changes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|change| json!({ "path": change["path"], "kind": change["kind"]["type"] }))
                .collect();
            json!({ "id": id, "type": "file_change", "changes": changes, "status": status(item) })
        }
        "mcpToolCall" => json!({
            "id": id, "type": "mcp_tool_call", "server": item["server"], "tool": item["tool"],
            "arguments": item["arguments"], "result": item["result"], "error": item["error"], "status": status(item),
        }),
        "webSearch" => json!({ "id": id, "type": "web_search", "query": item["query"] }),
        _ => return None,
    })
}

impl Translate {
    /// The events one notification stands for: streamed text directly, everything else through `Codex::parse_line`.
    fn events(&mut self, provider: &dyn Provider, method: &str, params: &Value) -> Vec<AgentEvent> {
        let item_id = params["itemId"].as_str().unwrap_or_default().to_owned();
        let delta = params["delta"].as_str().unwrap_or_default();
        match method {
            "item/agentMessage/delta" if !delta.is_empty() => {
                let first = self.streamed.insert(item_id);
                let text = if first && self.after_text { format!("\n\n{delta}") } else { delta.to_owned() };
                self.after_text = true;
                return vec![AgentEvent::Delta { text }];
            }
            "item/reasoning/summaryTextDelta" | "item/reasoning/textDelta" if !delta.is_empty() => {
                self.streamed.insert(item_id);
                self.after_text = false;
                return vec![AgentEvent::Thinking { text: delta.to_owned() }];
            }
            // A new part of the reasoning summary is a new paragraph.
            "item/reasoning/summaryPartAdded" if params["summaryIndex"].as_u64().unwrap_or(0) > 0 => {
                return vec![AgentEvent::Thinking { text: "\n\n".to_owned() }];
            }
            _ => {}
        }
        self.lines(method, params)
            .into_iter()
            .flat_map(|line| provider.parse_line(&line.to_string()))
            .collect()
    }

    /// The exec-format lines one notification stands for (none for most of them).
    fn lines(&mut self, method: &str, params: &Value) -> Vec<Value> {
        match method {
            "item/started" | "item/completed" => {
                let kind = params["item"]["type"].as_str().unwrap_or_default();
                if method == "item/started" && !matches!(kind, "agentMessage" | "userMessage") {
                    self.after_text = false;
                }
                let id = params["item"]["id"].as_str().unwrap_or_default();
                if matches!(kind, "agentMessage" | "reasoning") && self.streamed.contains(id) {
                    return Vec::new();
                }
                let Some(item) = exec_item(&params["item"]) else { return Vec::new() };
                if method == "item/completed" {
                    self.output.remove(id);
                    if kind == "agentMessage" {
                        self.after_text = true;
                    }
                }
                let kind = if method == "item/started" { "item.started" } else { "item.completed" };
                vec![json!({ "type": kind, "item": item })]
            }
            "item/commandExecution/outputDelta" => {
                let (Some(id), Some(delta)) = (params["itemId"].as_str(), params["delta"].as_str()) else { return Vec::new() };
                let all = self.output.entry(id.to_owned()).or_default();
                all.push_str(delta);
                vec![json!({ "type": "item.updated", "item": { "id": id, "type": "command_execution", "aggregated_output": all } })]
            }
            "thread/tokenUsage/updated" => {
                let last = &params["tokenUsage"]["last"];
                let count = |key: &str| last[key].as_u64().unwrap_or(0);
                self.input += count("inputTokens");
                self.cached += count("cachedInputTokens");
                self.written += count("cacheWriteInputTokens");
                self.out += count("outputTokens");
                Vec::new()
            }
            "error" => {
                let message = params["error"]["message"].as_str().unwrap_or("Codex failed");
                if params["willRetry"].as_bool().unwrap_or(false) {
                    vec![json!({ "type": "error", "message": format!("Reconnecting... 1/1 ({message})") })]
                } else {
                    vec![json!({ "type": "turn.failed", "error": { "message": message } })]
                }
            }
            "turn/completed" => match params["turn"]["status"].as_str() {
                Some("failed") => {
                    let message = params["turn"]["error"]["message"].as_str().unwrap_or("Codex failed");
                    vec![json!({ "type": "turn.failed", "error": { "message": message } })]
                }
                Some("interrupted") => vec![json!({ "type": "turn.failed", "error": { "message": "Codex stopped the turn" } })],
                _ => vec![json!({ "type": "turn.completed", "usage": {
                    "input_tokens": self.input, "cached_input_tokens": self.cached,
                    "cache_write_input_tokens": self.written, "output_tokens": self.out,
                }})],
            },
            _ => Vec::new(),
        }
    }
}

/// One JSON-RPC line to the server.
async fn write(warm: &mut Warm, message: &Value) -> std::io::Result<()> {
    send(warm, &format!("{message}\n")).await
}

/// Sends a request and reads until its reply (other lines, such as notifications, are skipped).
async fn request(warm: &mut Warm, method: &str, params: Value) -> Result<Value, String> {
    warm.rpc += 1;
    let id = warm.rpc;
    write(warm, &json!({ "id": id, "method": method, "params": params })).await.map_err(|error| format!("could not talk to Codex: {error}"))?;
    let wait = async {
        loop {
            let Ok(Some(bytes)) = warm.lines.next_segment().await else {
                return Err("Codex closed before it answered".to_owned());
            };
            let Ok(reply) = serde_json::from_slice::<Value>(&bytes) else { continue };
            if reply["id"] == id && reply.get("method").is_none() {
                return match reply.get("error") {
                    Some(error) => Err(error["message"].as_str().unwrap_or("Codex refused the request").to_owned()),
                    None => Ok(reply["result"].clone()),
                };
            }
        }
    };
    tokio::time::timeout(HANDSHAKE, wait).await.unwrap_or_else(|_| Err(format!("Codex did not answer {method}")))
}

/// Opens the thread a turn goes to on a running server: a new one, or the one `resume` names.
async fn thread(warm: &mut Warm, mut params: Value, resume: Option<&str>) -> Result<(), String> {
    let reply = match resume {
        Some(thread) => {
            params["threadId"] = json!(thread);
            // The server that held the thread before may still be going down (a stopped turn kills it a moment
            // later): Codex refuses a second writer until it has.
            let mut tries = 0;
            loop {
                match request(warm, "thread/resume", params.clone()).await {
                    Err(message) if message.contains("active writer") && tries < 6 => {
                        tries += 1;
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    other => break other?,
                }
            }
        }
        None => {
            params["ephemeral"] = json!(false);
            request(warm, "thread/start", params).await?
        }
    };
    let thread = reply["thread"]["id"].as_str().ok_or("Codex gave no thread")?;
    warm.conversation = Some(thread.to_owned());
    Ok(())
}

/// The conversation a turn continues: the thread it names, when it resumes one.
fn wanted(req: &TurnRequest) -> Option<&str> {
    req.session_id.as_deref().filter(|_| req.resume)
}

/// The settings key of a request, and the arguments its server starts with.
fn identity(provider: &dyn Provider, req: &TurnRequest, params: &Value) -> (String, Vec<String>, Vec<(String, String)>) {
    let env = provider.environment(req);
    let key = settings_key(provider.id(), &["app-server".to_owned(), params.to_string()], &env);
    (key, vec!["app-server".to_owned()], env)
}

/// A server started and opened on the chat's thread.
async fn launch(binary: OsString, provider: &dyn Provider, req: &TurnRequest, params: &Value, key: &str, args: &[String], env: &[(String, String)]) -> Result<Warm, String> {
    let mut warm = start(binary, provider, args, env, req, key)?;
    request(&mut warm, "initialize", json!({ "clientInfo": { "name": "agentdeck", "title": "AgentDeck", "version": "1" }, "capabilities": { "experimentalApi": true } })).await?;
    write(&mut warm, &json!({ "method": "initialized" })).await.map_err(|error| error.to_string())?;
    thread(&mut warm, params.clone(), wanted(req)).await?;
    Ok(warm)
}

/// Starts the chat's server before its message is sent (the user is typing), so the first turn does not wait for
/// Codex, its MCP servers and its thread. Does nothing when the chat already has one that fits.
pub async fn prewarm(binary: OsString, provider: &dyn Provider, req: &TurnRequest) {
    let Some(chat) = req.owner.clone() else { return };
    if provider.prepare(req).is_err() {
        return;
    }
    let lock = lock_of(&chat);
    let _turn = lock.lock().await;
    let params = thread_params(req);
    let (key, args, env) = identity(provider, req, &params);
    // A server with these settings is enough: the turn opens its thread on it in a fraction of a second.
    if holds(&chat, &key, None, true) {
        return;
    }
    if let Ok(mut warm) = launch(binary, provider, req, &params, &key, &args, &env).await {
        warm.fresh = true;
        park(chat, warm);
    }
}

/// How the turn ended.
enum Ending {
    Done,
    Failed,
    Closed,
}

/// Reads notifications until the turn is over, answering what the server asks of the client. A stop (`stop`) sends
/// `turn/interrupt` (as soon as the turn's id is known) and keeps reading: the turn ends as interrupted and the server
/// stays up. One silent for the deadline is given up (`Failed`: the caller ends it).
async fn read_turn(warm: &mut Warm, provider: &dyn Provider, thread: &str, stop: StopToken, emit: &mut impl FnMut(AgentEvent)) -> Ending {
    let mut translate = Translate::default();
    let mut stopper = Interrupter::new(stop);
    // The id of this turn (from `turn/start`'s reply or `turn/started`), and the id of the interrupt request once sent.
    let mut turn: Option<String> = None;
    let mut interrupt_id: Option<u64> = None;
    let mut wants_interrupt = false;
    let mut deadline: Option<tokio::time::Instant> = None;
    let mut dog = Watchdog::new(provider.stall_limit());
    loop {
        if wants_interrupt && interrupt_id.is_none() {
            if let Some(turn) = &turn {
                warm.rpc += 1;
                interrupt_id = Some(warm.rpc);
                let _ = write(warm, &interrupt::codex_interrupt_request(warm.rpc, thread, turn)).await;
            }
        }
        let next = match deadline {
            Some(at) => tokio::time::timeout_at(at, warm.lines.next_segment()).await.unwrap_or(Ok(None)),
            None => tokio::select! {
                next = warm.lines.next_segment() => next,
                _ = warm.child.wait() => {
                    deadline = Some(tokio::time::Instant::now() + EXIT_GRACE);
                    continue;
                }
                _ = tokio::time::sleep_until(dog.deadline()) => {
                    emit(AgentEvent::Error { message: dog.message() });
                    return Ending::Failed;
                }
                signal = stopper.signal() => {
                    if signal == Signal::Ask {
                        wants_interrupt = true;
                        continue;
                    }
                    emit(interrupt::killed(Some(thread)));
                    return Ending::Failed;
                }
            },
        };
        let Ok(Some(bytes)) = next else { return Ending::Closed };
        dog.touch();
        let Ok(message) = serde_json::from_slice::<Value>(&bytes) else { continue };
        let method = message["method"].as_str().unwrap_or_default();
        if method.is_empty() {
            // The reply to the interrupt: the turn's own end follows as `turn/completed`, so nothing to do.
            if interrupt_id.is_some() && message.get("id").and_then(Value::as_u64) == interrupt_id {
                continue;
            }
            if let Some(id) = message["result"]["turn"]["id"].as_str() {
                turn.get_or_insert_with(|| id.to_owned());
            }
            // The reply to `turn/start`: only a refusal matters.
            if let Some(error) = message.get("error") {
                emit(AgentEvent::Error { message: error["message"].as_str().unwrap_or("Codex refused the message").to_owned() });
                return Ending::Failed;
            }
            continue;
        }
        if let Some(id) = message.get("id") {
            // The server asks something (an approval, a form): nothing here answers it, and it must not wait for ever.
            let reply = json!({ "id": id, "error": { "code": -32601, "message": "AgentDeck cannot answer this request" } });
            let _ = write(warm, &reply).await;
            continue;
        }
        let params = &message["params"];
        if method == "turn/started" && params["threadId"].as_str().is_none_or(|id| id == thread) {
            if let Some(id) = params["turn"]["id"].as_str() {
                turn.get_or_insert_with(|| id.to_owned());
            }
        }
        // Sub-agents run on threads of their own; their notifications are not this turn's.
        if params["threadId"].as_str().is_some_and(|other| other != thread) {
            continue;
        }
        let mut ending = None;
        for event in translate.events(provider, method, params) {
            dog.note(&event);
            let event = if stopper.asked() { interrupt::stopped(event, Some(thread)) } else { event };
            match &event {
                AgentEvent::Done { .. } => ending = Some(Ending::Done),
                // The server reports a failed turn as its own notification and stays up for the next turn: it is kept, as
                // a Claude process is (warm.rs), and only the turn is over.
                AgentEvent::Error { .. } => ending = Some(Ending::Done),
                _ => {}
            }
            emit(event);
        }
        if let Some(ending) = ending {
            return ending;
        }
    }
}

/// Runs one turn on the chat's Codex server (starting it if the chat has none that fits). `false`, having emitted
/// nothing, when the turn has no chat to keep a server for.
pub async fn drive(binary: OsString, provider: &dyn Provider, req: &TurnRequest, stop: StopToken, mut emit: impl FnMut(AgentEvent)) -> bool {
    let Some(chat) = req.owner.clone() else {
        return false;
    };
    if let Err(message) = provider.prepare(req) {
        emit(AgentEvent::Error { message });
        return true;
    }
    // Resets what the parser keeps for a turn (the folder a changed file is read from).
    let _ = provider.args(req);
    let lock = lock_of(&chat);
    let _turn = lock.lock().await;
    let params = thread_params(req);
    let (key, args, env) = identity(provider, req, &params);

    // A kept server with these settings takes the turn. On another conversation than the one it holds (a model change
    // starts a new one, a chat goes back to an older one) it opens that thread: no new process.
    let mut reused = take_server(&chat, &key);
    let mut warm = loop {
        let was_kept = reused.is_some();
        let switched = match reused.take() {
            Some(mut warm) => {
                let same = match wanted(req) {
                    Some(id) => warm.conversation.as_deref() == Some(id),
                    None => warm.fresh,
                };
                if same {
                    Ok(warm)
                } else {
                    thread(&mut warm, params.clone(), wanted(req)).await.map(|()| warm)
                }
            }
            None => Err(String::new()),
        };
        let mut warm = match switched {
            Ok(warm) => warm,
            Err(_) => match launch(binary.clone(), provider, req, &params, &key, &args, &env).await {
                Ok(warm) => warm,
                Err(message) => {
                    emit(AgentEvent::Error { message });
                    return true;
                }
            },
        };
        warm.fresh = false;
        let Some(thread) = warm.conversation.clone() else { return true };
        let mut input = json!({ "threadId": thread, "input": [{ "type": "text", "text": req.prompt, "text_elements": [] }] });
        if let Some(model) = &req.model {
            input["model"] = json!(model);
        }
        if let Some(effort) = &req.effort {
            input["effort"] = json!(effort);
        }
        warm.rpc += 1;
        let message = json!({ "id": warm.rpc, "method": "turn/start", "params": input });
        match write(&mut warm, &message).await {
            Ok(()) => break warm,
            // A kept server may have ended while it waited: the next round starts a new one.
            Err(_) if was_kept => continue,
            Err(error) => {
                emit(AgentEvent::Error { message: format!("could not send the message to Codex: {error}") });
                return true;
            }
        }
    };

    let thread = warm.conversation.clone().unwrap_or_default();
    // The parser's thread id for the final event.
    for event in provider.parse_line(&json!({ "type": "thread.started", "thread_id": thread }).to_string()) {
        emit(event);
    }
    match read_turn(&mut warm, provider, &thread, stop, &mut emit).await {
        Ending::Done => park(chat, warm),
        Ending::Failed => drop(warm),
        Ending::Closed => {
            let stderr = warm.stderr.lock().map(|kept| String::from_utf8_lossy(&kept).trim().to_owned()).unwrap_or_default();
            drop(warm);
            emit(AgentEvent::Error { message: if stderr.is_empty() { "agent ended without a result".to_owned() } else { stderr } });
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::codex::Codex;
    use super::*;

    fn req(mode: &str) -> TurnRequest {
        TurnRequest {
            prompt: "hi".into(),
            session_id: None,
            resume: false,
            model: Some("m".into()),
            permission_mode: Some(mode.into()),
            effort: Some("high".into()),
            allowed_tools: Vec::new(),
            cwd: None,
            owner: Some("chat".into()),
            api_key: None,
        }
    }

    #[test]
    fn items_become_exec_lines_and_usage_adds_up() {
        let mut translate = Translate::default();
        let started = json!({ "item": { "type": "commandExecution", "id": "c1", "command": "echo hi", "status": "inProgress" } });
        let lines = translate.lines("item/started", &started);
        assert_eq!(lines[0]["item"]["type"], "command_execution");
        assert_eq!(lines[0]["item"]["status"], "in_progress");
        let delta = json!({ "itemId": "c1", "delta": "hi" });
        assert_eq!(translate.lines("item/commandExecution/outputDelta", &delta)[0]["item"]["aggregated_output"], "hi");
        for total in [10, 5] {
            translate.lines("thread/tokenUsage/updated", &json!({ "tokenUsage": { "last": { "inputTokens": total, "cachedInputTokens": 2, "outputTokens": 1 } } }));
        }
        let done = translate.lines("turn/completed", &json!({ "turn": { "status": "completed" } }));
        assert_eq!(done[0]["type"], "turn.completed");
        assert_eq!(done[0]["usage"]["input_tokens"], 15);
        assert_eq!(done[0]["usage"]["cached_input_tokens"], 4);
        let failed = translate.lines("turn/completed", &json!({ "turn": { "status": "failed", "error": { "message": "no" } } }));
        assert_eq!(failed[0]["error"]["message"], "no");
    }

    #[test]
    fn answer_text_streams_once_with_paragraphs_between_messages() {
        let codex = Codex::default();
        let mut translate = Translate::default();
        let mut text = String::new();
        let mut said = |method: &str, params: Value, translate: &mut Translate| {
            for event in translate.events(&codex, method, &params) {
                if let AgentEvent::Delta { text: delta } = event {
                    text.push_str(&delta);
                }
            }
        };
        said("item/agentMessage/delta", json!({ "itemId": "a", "delta": "one" }), &mut translate);
        said("item/completed", json!({ "item": { "type": "agentMessage", "id": "a", "text": "one" } }), &mut translate);
        said("item/agentMessage/delta", json!({ "itemId": "b", "delta": "two" }), &mut translate);
        assert_eq!(text, "one\n\ntwo", "streamed once, the finished copy is not repeated");
    }

    #[test]
    fn model_and_effort_stay_out_of_the_server_settings() {
        let mut other = req("acceptEdits");
        other.model = Some("n".into());
        other.effort = Some("low".into());
        assert_eq!(thread_params(&req("acceptEdits")), thread_params(&other), "a new model or effort keeps the server");
        let auto = thread_params(&req("auto"));
        assert_eq!(auto["approvalsReviewer"], "auto_review");
        assert_eq!(auto["sandbox"], "workspace-write");
    }
}
