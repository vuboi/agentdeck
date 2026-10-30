//! `delegate_task` and its companions on the app's `agentdeck` MCP server: an agent (Claude, Codex, Antigravity or a
//! local model) hands a task to another agent during its own turn, as a tool call.
//!
//! The work is done by the webview, which owns the chats: each call is emitted as an `agent-delegate` event
//! `{ callId, tool, caller, args }` (`caller` is the chat the MCP URL names, see `mcp_url`) and the call waits until the
//! webview answers through the `delegate_reply` command. Nothing here knows how a child chat runs. A call that is dropped
//! before its answer (the caller's run was stopped, its connection closed) tells the webview with `agent-delegate-abort`,
//! so a worker nobody waits for is stopped.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

pub const DEFAULT_TIMEOUT_S: u64 = 1800;
pub const MIN_TIMEOUT_S: u64 = 10;
pub const MAX_TIMEOUT_S: u64 = 7200;
/// Longest task text accepted: it is a prompt, not a document.
const TASK_MAX_CHARS: usize = 30_000;
/// How long a quick tool (list, status, stop, a task started in the background) may wait for the webview.
const QUICK_WAIT: Duration = Duration::from_secs(20);
/// What the call waits beyond the task's own timeout, which the webview enforces (stopping the worker): only a backstop.
const BACKSTOP: Duration = Duration::from_secs(60);

pub const DELEGATE_TOOL: &str = "delegate_task";
const TOOLS: [&str; 4] = [DELEGATE_TOOL, "list_agents", "task_status", "task_stop"];

pub fn is_tool(name: &str) -> bool {
    TOOLS.contains(&name)
}

pub fn tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": DELEGATE_TOOL,
            "description": format!("Hands a task to another AI agent running in AgentDeck (Claude, Codex, Antigravity or a local model) and returns its short report. Use this for cross-provider handoffs or independent work that benefits from another AgentDeck agent. If your CLI already has native sub-agents, use those for its own internal orchestration; do not replace them with this cross-provider tool. Do NOT delegate small things: keep ordinary Q&A, a small edit, a quick command or a simple artifact action in the current chat, and do not invent work just to delegate. Use 1-4 workers for genuinely independent capabilities such as investigation, implementation on disjoint files, verification, browser or artifact work; never assign overlapping write scopes. Give each worker a self-contained goal, paths/resources, constraints and evidence-first handoff: it sees nothing of this conversation. Independent tasks go out together in ONE turn with wait=false so they work in the background while you continue the main task; use wait=true only when the next step depends on a report. At most {MAX_PARALLEL_WORKERS} workers run at a time; more wait in line and start as others finish. Keep ownership of the main task and final integration. Never delegate merely to relay the user's message; never delegate the final answer. Reuse worker conversations for corrections: to send a correction to a worker that already answered, call delegate_task with continue=<task_id>. Replan only after evidence shows a real gap. Call list_agents first when unsure which agent or model to use. A FREE TIER agent sees no file or project and may train on the task: send it only public, non-sensitive text."),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "agent": { "type": "string", "description": "claude, codex, antigravity, or the id of a local model provider as listed by list_agents" },
                    "continue": { "type": "string", "description": "a task_id (e.g. t-ab12cd) or worker chat id of a worker that already answered and is idle: its chat is reused and `task` is sent to it as a follow-up. agent and model may then be left out" },
                    "model": { "type": "string", "description": "a model of that agent as listed by list_agents (a part of its name is enough); left out: the agent's usual one" },
                    "task": { "type": "string", "description": "the self-contained task: goal, paths, constraints, what to report back" },
                    "wait": { "type": "boolean", "description": "true (default): wait for the report because the next step depends on it. false: run in the background and return a task_id now" },
                    "timeout_s": { "type": "number", "description": format!("seconds the worker may take before it is stopped (default {DEFAULT_TIMEOUT_S}, at most {MAX_TIMEOUT_S})") },
                },
                "required": ["task"],
            },
        }),
        json!({
            "name": "list_agents",
            "description": "The agents delegate_task can hand work to right now, with their models and the id to pass as `agent`. Use these for cross-provider handoffs or extra independent work; if your CLI has native sub-agents, use them for its internal orchestration. Independent AgentDeck work can run in the background while you continue. Keep small things in the main chat and keep the final answer yourself. An agent marked FREE TIER may train on what it is sent: give it only light public work (search, emails, prompts, scripts, small text edits) with no source code, private data or secrets.",
            "inputSchema": { "type": "object", "properties": {}, "required": [] },
        }),
        json!({
            "name": "task_status",
            "description": "The state of a task started with delegate_task (queued, running, done, failed or stopped), with its report once done. A background task's report is delivered to you anyway: use this only to look before it arrives.",
            "inputSchema": { "type": "object", "properties": { "task_id": { "type": "string", "description": "the task_id delegate_task returned" } }, "required": ["task_id"] },
        }),
        json!({
            "name": "task_stop",
            "description": "Stops a task started with delegate_task: its worker is killed. Use it when the result is no longer needed.",
            "inputSchema": { "type": "object", "properties": { "task_id": { "type": "string", "description": "the task_id delegate_task returned" } }, "required": ["task_id"] },
        }),
    ]
}

/// The arguments of `delegate_task`, checked and with their defaults.
#[derive(Debug, PartialEq)]
pub struct DelegateArgs {
    pub agent: String,
    pub model: Option<String>,
    pub task: String,
    pub wait: bool,
    pub timeout_s: u64,
    /// A task id or worker chat id whose chat is reused for a follow-up.
    pub continue_id: Option<String>,
}

fn text_of(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Local models often send `"true"` or `1` for a boolean.
fn flag_of(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Some(true),
            "false" | "no" | "0" => Some(false),
            _ => None,
        },
        Value::Number(number) => number.as_f64().map(|n| n != 0.0),
        _ => None,
    }
}

fn seconds_of(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}

pub fn parse_delegate(args: &Value) -> Result<DelegateArgs, String> {
    let continue_id = text_of(&args["continue"]);
    let agent = match text_of(&args["agent"]) {
        Some(agent) => agent,
        None if continue_id.is_some() => String::new(),
        None => return Err("agent is required (unless continue is given): claude, codex, antigravity or an id from list_agents".into()),
    };
    let task = text_of(&args["task"])
        .ok_or("task is required: say what the worker must do and report back")?;
    if task.chars().count() > TASK_MAX_CHARS {
        return Err(format!("task is longer than {TASK_MAX_CHARS} characters: give the goal and the paths, not whole files"));
    }
    let wait = match args.get("wait") {
        None | Some(Value::Null) => true,
        Some(value) => flag_of(value).ok_or("wait must be true or false")?,
    };
    let timeout_s = match args.get("timeout_s") {
        None | Some(Value::Null) => DEFAULT_TIMEOUT_S,
        Some(value) => {
            let seconds = seconds_of(value)
                .filter(|seconds| seconds.is_finite())
                .ok_or("timeout_s must be a number of seconds")?;
            (seconds.round().max(0.0) as u64).clamp(MIN_TIMEOUT_S, MAX_TIMEOUT_S)
        }
    };
    Ok(DelegateArgs {
        agent,
        model: text_of(&args["model"]),
        task,
        wait,
        timeout_s,
        continue_id,
    })
}

pub fn parse_task_id(args: &Value) -> Result<String, String> {
    let id =
        text_of(&args["task_id"]).ok_or("task_id is required: the one delegate_task returned")?;
    if id.len() > 64
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(format!("{id} is not a task_id"));
    }
    Ok(id)
}

/// How long the call waits for the webview's answer.
pub fn wait_limit(tool: &str, args: &Value) -> Duration {
    match (tool, parse_delegate(args)) {
        (DELEGATE_TOOL, Ok(parsed)) if parsed.wait => {
            Duration::from_secs(parsed.timeout_s) + BACKSTOP
        }
        _ => QUICK_WAIT,
    }
}

/// Whether a JSON-RPC body is a `delegate_task` call that blocks until the worker is done: its answer is streamed so
/// the connection is not taken for dead (an HTTP client gives up on a response that says nothing for minutes).
pub fn is_long_call(body: &str) -> bool {
    let Ok(request) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    request["method"] == "tools/call"
        && request["params"]["name"] == DELEGATE_TOOL
        && parse_delegate(&request["params"]["arguments"]).is_ok_and(|parsed| parsed.wait)
}

// ---------------------------------------------------------------------------------------------------------------
// Calls waiting for the webview

/// The webview's answer to a call.
#[derive(Debug, PartialEq)]
pub struct Reply {
    pub text: String,
    pub is_error: bool,
}

static NEXT: AtomicU64 = AtomicU64::new(1);
static PENDING: LazyLock<Mutex<HashMap<String, oneshot::Sender<Reply>>>> =
    LazyLock::new(Default::default);

/// The calls waiting for the webview. A panic while it was held must not stop later calls from being answered: the map is
/// still sound, so its guard is taken back.
fn pending() -> std::sync::MutexGuard<'static, HashMap<String, oneshot::Sender<Reply>>> {
    PENDING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The cap on running workers. Taken back after a panic too: the gate fails open, never stuck.
fn gate() -> std::sync::MutexGuard<'static, Gate> {
    GATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn register() -> (String, oneshot::Receiver<Reply>) {
    let id = format!("dlg-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let (sender, receiver) = oneshot::channel();
    pending().insert(id.clone(), sender);
    (id, receiver)
}

fn resolve(id: &str, reply: Reply) -> bool {
    let sender = pending().remove(id);
    sender.is_some_and(|sender| sender.send(reply).is_ok())
}

fn fail_all(reason: &str) -> usize {
    let all: Vec<_> = pending().drain().collect();
    let count = all.len();
    for (_, sender) in all {
        let _ = sender.send(Reply {
            text: reason.to_owned(),
            is_error: true,
        });
    }
    count
}

/// Removes the call when it is dropped without an answer, and tells the webview to stop the worker nobody waits for.
struct Abandon<'a> {
    app: &'a AppHandle,
    id: String,
    notify: bool,
    done: bool,
}

impl Drop for Abandon<'_> {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        pending().remove(&self.id);
        if self.notify {
            let _ = self
                .app
                .emit_to("main", "agent-delegate-abort", json!({ "callId": self.id }));
        }
    }
}

fn reply_value(text: String, is_error: bool) -> Value {
    if is_error {
        json!({ "content": [{ "type": "text", "text": text }], "isError": true })
    } else {
        json!({ "content": [{ "type": "text", "text": text }] })
    }
}

/// One tool call of the MCP server: checks the arguments, hands the call to the webview and waits for its answer.
pub async fn call(app: &AppHandle, tool: &str, args: &Value, chat: &str) -> Value {
    let checked = match tool {
        DELEGATE_TOOL => parse_delegate(args).map(|parsed| {
            json!({ "agent": parsed.agent, "model": parsed.model, "task": parsed.task, "wait": parsed.wait, "timeout_s": parsed.timeout_s, "continue": parsed.continue_id })
        }),
        "task_status" | "task_stop" => parse_task_id(args).map(|id| json!({ "task_id": id })),
        _ => Ok(json!({})),
    };
    let args = match checked {
        Ok(args) => args,
        Err(error) => return reply_value(error, true),
    };
    let limit = wait_limit(tool, &args);
    let (id, receiver) = register();
    let mut guard = Abandon {
        app,
        id: id.clone(),
        notify: tool == DELEGATE_TOOL && args["wait"] == true,
        done: false,
    };
    // Only the main window: a Preview page is a copy of this app and must not answer the chats' calls.
    if let Err(error) = app.emit_to(
        "main",
        "agent-delegate",
        json!({ "callId": id, "tool": tool, "caller": chat, "args": args }),
    ) {
        return reply_value(format!("AgentDeck could not start the task: {error}"), true);
    }
    match tokio::time::timeout(limit, receiver).await {
        Ok(Ok(reply)) => {
            guard.done = true;
            reply_value(reply.text, reply.is_error)
        }
        Ok(Err(_)) => {
            guard.done = true;
            reply_value("AgentDeck dropped the call.".into(), true)
        }
        Err(_) => reply_value(
            format!("AgentDeck did not answer within {} s.", limit.as_secs()),
            true,
        ),
    }
}

/// The webview's answer to a call (the worker's report, or the reason there is none).
#[tauri::command]
pub fn delegate_reply(call_id: String, text: String, is_error: bool) -> bool {
    resolve(&call_id, Reply { text, is_error })
}

/// The webview just (re)loaded: the chats and workers of the page before it are gone, so no call can be answered.
#[tauri::command]
pub fn delegate_ready() -> usize {
    *gate() = Gate::default();
    fail_all("AgentDeck was reloaded while this task ran; the task is lost. Start it again if it is still needed.")
}

// ---------------------------------------------------------------------------------------------------------------
// The cap on workers running at once

/// How many delegated workers run at the same time for one asking chat (wait=true or false). The cap is per chat: a
/// chat whose workers hang or pile up never takes a place from another chat. The webview asks for a place before it
/// starts a worker (`delegate_enter`) and gives it back when the task ends (`delegate_release`).
pub const MAX_PARALLEL_WORKERS: usize = 4;

/// Workers running and workers waiting for a place, in the order they asked, each with the chat that asked. Holds ids
/// only, no I/O: a place is given by `enter` or, once one is released, by the list `release` returns.
#[derive(Debug, Default)]
struct Gate {
    running: Vec<(String, String)>,
    waiting: VecDeque<(String, String)>,
}

impl Gate {
    fn used_by(&self, owner: &str) -> usize {
        self.running.iter().filter(|(_, who)| who == owner).count()
    }

    /// `true`: the task may start now. `false`: it waits, and `release` names it when its turn comes.
    fn enter(&mut self, id: &str, owner: &str) -> bool {
        if self.running.iter().any(|(item, _)| item == id) {
            return true;
        }
        if self.waiting.iter().any(|(item, _)| item == id) {
            return false;
        }
        if self.used_by(owner) < MAX_PARALLEL_WORKERS {
            self.running.push((id.to_owned(), owner.to_owned()));
            return true;
        }
        self.waiting.push_back((id.to_owned(), owner.to_owned()));
        false
    }

    /// Frees the task's place (or its place in the line) and returns the waiting tasks that may start now.
    fn release(&mut self, id: &str) -> Vec<String> {
        self.running.retain(|(item, _)| item != id);
        self.waiting.retain(|(item, _)| item != id);
        let mut started = Vec::new();
        let mut index = 0;
        while index < self.waiting.len() {
            let owner = self.waiting[index].1.clone();
            if self.used_by(&owner) < MAX_PARALLEL_WORKERS {
                if let Some(entry) = self.waiting.remove(index) {
                    started.push(entry.0.clone());
                    self.running.push(entry);
                }
            } else {
                index += 1;
            }
        }
        started
    }
}

static GATE: LazyLock<Mutex<Gate>> = LazyLock::new(Default::default);

/// A worker is about to start: `true` when it may, `false` when it is queued behind `MAX_PARALLEL_WORKERS` others (the
/// webview is then told with `agent-delegate-slot` `{ taskId }` when its turn comes).
#[tauri::command]
pub fn delegate_enter(task_id: String, owner: Option<String>) -> bool {
    let owner = owner.unwrap_or_default();
    gate().enter(&task_id, &owner)
}

/// A task ended (done, failed, stopped, timed out or never started): its place goes to the next one waiting.
#[tauri::command]
pub fn delegate_release(app: AppHandle, task_id: String) {
    let started = gate().release(&task_id);
    for id in started {
        let _ = app.emit_to("main", "agent-delegate-slot", json!({ "taskId": id }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_wait_and_thirty_minutes() {
        let parsed = parse_delegate(&json!({ "agent": " claude ", "task": " read the logs " }))
            .expect("valid");
        assert_eq!(
            parsed,
            DelegateArgs {
                agent: "claude".into(),
                model: None,
                task: "read the logs".into(),
                wait: true,
                timeout_s: 1800,
                continue_id: None
            }
        );
    }

    #[test]
    fn missing_or_empty_agent_and_task_are_refused() {
        assert!(parse_delegate(&json!({ "task": "x" })).is_err());
        assert!(parse_delegate(&json!({ "agent": "codex" })).is_err());
        assert!(parse_delegate(&json!({ "agent": "codex", "task": "   " })).is_err());
        assert!(parse_delegate(&json!({ "agent": 3, "task": "x" })).is_err());
        assert!(parse_delegate(&Value::Null).is_err());
    }

    #[test]
    fn continue_replaces_agent() {
        let parsed = parse_delegate(&json!({ "continue": " t-ab12cd ", "task": "fix it" })).expect("valid");
        assert_eq!(parsed.continue_id.as_deref(), Some("t-ab12cd"));
        assert!(parsed.agent.is_empty());
        assert!(parse_delegate(&json!({ "continue": "   ", "task": "x" })).is_err());
        assert!(parse_delegate(&json!({ "continue": "t-ab12cd" })).is_err());
    }

    #[test]
    fn wait_and_timeout_accept_what_local_models_send() {
        let args = json!({ "agent": "codex", "task": "x", "wait": "false", "timeout_s": "90", "model": "gpt-6-luna" });
        let parsed = parse_delegate(&args).expect("valid");
        assert!(!parsed.wait);
        assert_eq!(parsed.timeout_s, 90);
        assert_eq!(parsed.model.as_deref(), Some("gpt-6-luna"));
        assert!(parse_delegate(&json!({ "agent": "a", "task": "x", "wait": "maybe" })).is_err());
        assert!(
            parse_delegate(&json!({ "agent": "a", "task": "x", "timeout_s": "soon" })).is_err()
        );
        assert!(
            parse_delegate(&json!({ "agent": "a", "task": "x", "wait": null }))
                .expect("null is unset")
                .wait
        );
    }

    #[test]
    fn timeout_is_kept_within_bounds() {
        let timeout = |value: Value| {
            parse_delegate(&json!({ "agent": "a", "task": "x", "timeout_s": value }))
                .expect("valid")
                .timeout_s
        };
        assert_eq!(timeout(json!(1)), MIN_TIMEOUT_S);
        assert_eq!(timeout(json!(-5)), MIN_TIMEOUT_S);
        assert_eq!(timeout(json!(999_999)), MAX_TIMEOUT_S);
        assert_eq!(timeout(json!(600.4)), 600);
    }

    #[test]
    fn a_huge_task_is_refused() {
        let task = "x".repeat(TASK_MAX_CHARS + 1);
        assert!(parse_delegate(&json!({ "agent": "claude", "task": task })).is_err());
    }

    #[test]
    fn task_ids_are_plain_words() {
        assert_eq!(
            parse_task_id(&json!({ "task_id": " t-ab12 " })).as_deref(),
            Ok("t-ab12")
        );
        assert!(parse_task_id(&json!({})).is_err());
        assert!(parse_task_id(&json!({ "task_id": "a b" })).is_err());
        assert!(parse_task_id(&json!({ "task_id": "../x" })).is_err());
    }

    #[test]
    fn the_wait_limit_follows_the_task_timeout() {
        let blocking = json!({ "agent": "claude", "task": "x", "timeout_s": 120 });
        assert_eq!(
            wait_limit(DELEGATE_TOOL, &blocking),
            Duration::from_secs(120) + BACKSTOP
        );
        assert_eq!(
            wait_limit(DELEGATE_TOOL, &json!({ "agent": "claude", "task": "x" })),
            Duration::from_secs(1800) + BACKSTOP
        );
        // A background task answers at once; so do the other tools.
        assert_eq!(
            wait_limit(
                DELEGATE_TOOL,
                &json!({ "agent": "claude", "task": "x", "wait": false })
            ),
            QUICK_WAIT
        );
        assert_eq!(
            wait_limit("task_status", &json!({ "task_id": "t1" })),
            QUICK_WAIT
        );
        assert_eq!(wait_limit("list_agents", &json!({})), QUICK_WAIT);
    }

    #[test]
    fn only_a_blocking_delegate_call_is_streamed() {
        let call = |name: &str, arguments: Value| {
            json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": arguments } }).to_string()
        };
        assert!(is_long_call(&call(
            DELEGATE_TOOL,
            json!({ "agent": "claude", "task": "x" })
        )));
        assert!(!is_long_call(&call(
            DELEGATE_TOOL,
            json!({ "agent": "claude", "task": "x", "wait": false })
        )));
        assert!(!is_long_call(&call(
            DELEGATE_TOOL,
            json!({ "agent": "claude" })
        )));
        assert!(!is_long_call(&call(
            "task_status",
            json!({ "task_id": "t1" })
        )));
        assert!(!is_long_call(&call(
            "browser_click",
            json!({ "x": 1, "y": 2 })
        )));
        assert!(!is_long_call(
            &json!({ "method": "tools/list" }).to_string()
        ));
        assert!(!is_long_call("not json"));
    }

    #[test]
    fn workers_run_up_to_the_limit_and_the_next_waits_for_a_release() {
        // Written against the limit, not a number: it is raised and lowered in one place.
        let mut gate = Gate::default();
        let running: Vec<String> = (0..MAX_PARALLEL_WORKERS).map(|n| format!("w{n}")).collect();
        for id in &running {
            assert!(gate.enter(id, "chat"));
        }
        assert!(!gate.enter("x", "chat"));
        assert!(!gate.enter("y", "chat"));
        // Asking twice does not take a second place, nor lose the one in line.
        assert!(gate.enter(&running[0], "chat"));
        assert!(!gate.enter("x", "chat"));
        assert_eq!(gate.release(&running[1]), ["x"]);
        assert_eq!(gate.running.len(), MAX_PARALLEL_WORKERS);
        // Completion order, not start order: the next place goes to the next in line.
        assert_eq!(gate.release(&running[0]), ["y"]);
        assert!(gate.release(&running[2]).is_empty());
        assert_eq!(gate.running.len(), MAX_PARALLEL_WORKERS - 1);
    }

    #[test]
    fn one_chat_full_of_workers_does_not_block_another_chat() {
        let mut gate = Gate::default();
        for n in 0..MAX_PARALLEL_WORKERS {
            assert!(gate.enter(&format!("a{n}"), "chat-a"));
        }
        assert!(!gate.enter("a-extra", "chat-a"));
        assert!(gate.enter("b0", "chat-b"));
        // A place freed in chat B never starts chat A's waiting task.
        assert!(gate.release("b0").is_empty());
        assert_eq!(gate.release("a0"), ["a-extra"]);
    }

    #[test]
    fn a_task_stopped_in_line_leaves_the_line_and_an_unknown_release_is_harmless() {
        let mut gate = Gate::default();
        let running: Vec<String> = (0..MAX_PARALLEL_WORKERS).map(|n| format!("w{n}")).collect();
        for id in &running {
            assert!(gate.enter(id, "chat"));
        }
        assert!(!gate.enter("x", "chat"));
        assert!(!gate.enter("y", "chat"));
        assert!(gate.release("x").is_empty());
        assert!(gate.release("nobody").is_empty());
        assert_eq!(gate.release(&running[0]), ["y"]);
    }

    #[test]
    fn the_tools_are_listed_with_their_guidance() {
        let defs = tool_defs();
        let names: Vec<_> = defs
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        assert_eq!(
            names,
            ["delegate_task", "list_agents", "task_status", "task_stop"]
        );
        assert!(names.iter().all(|name| is_tool(name)));
        let text = defs[0]["description"].as_str().unwrap_or_default();
        for phrase in [
            "wait=true",
            "wait=false",
            "Do NOT delegate small things",
            "never delegate the final answer",
            "1-4 workers",
            "continue=<task_id>",
        ] {
            assert!(text.contains(phrase), "the description lacks: {phrase}");
        }
        assert_eq!(defs[0]["inputSchema"]["required"], json!(["task"]));
    }

    // One test: both use the shared registry, and a reload empties it for everyone.
    #[tokio::test]
    async fn an_answer_reaches_its_call_once_and_a_reload_fails_the_rest() {
        let (id, receiver) = register();
        assert!(resolve(
            &id,
            Reply {
                text: "report".into(),
                is_error: false
            }
        ));
        assert_eq!(
            receiver.await.ok(),
            Some(Reply {
                text: "report".into(),
                is_error: false
            })
        );
        // Nobody waits any more: a second answer is not taken.
        assert!(!resolve(
            &id,
            Reply {
                text: "again".into(),
                is_error: false
            }
        ));
        assert!(!resolve(
            "dlg-unknown",
            Reply {
                text: "x".into(),
                is_error: false
            }
        ));

        let (_, first) = register();
        let (_, second) = register();
        assert_eq!(fail_all("gone"), 2);
        for receiver in [first, second] {
            let reply = receiver.await.expect("answered");
            assert!(reply.is_error);
            assert_eq!(reply.text, "gone");
        }
    }
}
