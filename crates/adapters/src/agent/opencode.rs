//! OpenCode, driven headless with `opencode run --format json`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{json, Value};

use super::codex::APP_CONTEXT;
use super::permission::{self, Action, Mode, Verdict};
use super::{allow_entry, AgentEvent, PermissionRequest, Provider, TaskStatus, TurnRequest};

/// The label of a sub-agent call: `agent: description`, from whichever of the two the call has.
fn task_label(input: &Value) -> Option<String> {
    match (input["agent"].as_str(), input["description"].as_str()) {
        (Some(agent), Some(what)) => Some(format!("{agent}: {what}")),
        (Some(text), None) | (None, Some(text)) => Some(text.to_owned()),
        (None, None) => None,
    }
}

/// Whether a tool is a sub-agent call: `subagent` (the name the CLI writes) or `task` (the name the server gives it).
pub fn is_subagent(tool: &str) -> bool {
    matches!(tool, "subagent" | "task")
}

/// The task card of a sub-agent call, keyed by the call that started it. Used where the call's own metadata (the child
/// session) is not known, as in the server's event stream.
pub fn subagent_event(input: &Value, call: &str, status: TaskStatus, summary: Option<String>) -> AgentEvent {
    AgentEvent::Task {
        id: call.to_owned(),
        tool_use_id: Some(call.to_owned()),
        status,
        label: task_label(input),
        summary,
        tools: None,
    }
}

/// Where a `subagent` tool call stands. The child runs in a session of its own (`metadata.sessionID`); its text is not
/// streamed into this one, only its result comes back, so the call needs no scoping, just a label and a status.
fn subagent_task(part: &Value, call: &str, status: TaskStatus) -> AgentEvent {
    let state = &part["state"];
    let child = [&state["metadata"]["metadata"]["sessionID"], &state["metadata"]["sessionID"]]
        .into_iter()
        .find_map(Value::as_str)
        .unwrap_or(call);
    let failed = (status == TaskStatus::Failed).then(|| state["error"].as_str().map(str::to_owned)).flatten();
    AgentEvent::Task {
        id: child.to_owned(),
        tool_use_id: Some(call.to_owned()),
        status,
        label: task_label(&state["input"]),
        summary: failed,
        tools: None,
    }
}

/// What a turn has said so far: the text of its answer and what its steps cost. OpenCode writes text before a tool call
/// too (a preamble), so the answer is the text of the last step that wrote any, as the `result` of a Claude turn is.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Answer {
    /// The step in progress.
    step: String,
    /// The last step that ended with text.
    earlier: String,
    /// Sum of the steps' cost.
    cost: f64,
}

impl Answer {
    pub fn push_text(&mut self, text: &str) {
        self.step.push_str(text);
    }

    /// A step ended with this cost. Its text stays the answer until a later step writes some of its own.
    pub fn end_step(&mut self, cost: f64) {
        self.cost += cost;
        let text = std::mem::take(&mut self.step);
        if !text.trim().is_empty() {
            self.earlier = text;
        }
    }

    /// The answer: the step in progress, or else the last step that wrote text.
    pub fn text(&self) -> String {
        if self.step.trim().is_empty() {
            self.earlier.clone()
        } else {
            self.step.clone()
        }
    }

    /// What the turn cost, when it cost anything (a free model reports none).
    pub fn cost_usd(&self) -> Option<f64> {
        (self.cost > 0.0).then_some(self.cost)
    }
}

/// How a permission request of OpenCode is answered: approved once, or refused and reported as a card.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionAnswer {
    Approve,
    Refuse(PermissionRequest),
}

/// The kind of an action the server asks about, as the modes read it.
fn action_kind(action: &str) -> Action {
    match action {
        "shell" | "bash" => Action::Shell,
        "edit" | "write" | "patch" | "apply_patch" | "multiedit" => Action::Write,
        "read" | "glob" | "grep" | "list" | "lsp" => Action::Read,
        _ => Action::Other,
    }
}

/// The name of an action in an allow list (`--allowedTools`): `Bash` for a command, `Write` for a file change, `Read`.
fn allow_name(action: &str) -> String {
    match action_kind(action) {
        Action::Shell => "Bash".into(),
        Action::Write => "Write".into(),
        Action::Read => "Read".into(),
        Action::Other => action.to_owned(),
    }
}

/// What a permission request is about, in one line: the command, the file, the pattern.
fn request_summary(data: &Value) -> String {
    let metadata = &data["metadata"];
    ["command", "filepath", "filePath", "path", "pattern", "url"]
        .into_iter()
        .find_map(|key| metadata[key].as_str().filter(|text| !text.is_empty()))
        .map(str::to_owned)
        .or_else(|| {
            let resources: Vec<&str> = data["resources"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            (!resources.is_empty()).then(|| resources.join(", "))
        })
        .or_else(|| data["message"].as_str().filter(|text| !text.is_empty()).map(str::to_owned))
        .unwrap_or_else(|| data["action"].as_str().unwrap_or_default().to_owned())
}

/// Answers a permission request of the server the way Claude's `--allowedTools` and the modes do: a call the mode or the
/// approved list allows runs; any other is refused, and offered to the user as a card whose `allow` approves exactly it.
/// `data` is the request as the server sends it (`permission.asked`); `id` falls back to the request's own id.
pub fn answer_permission(data: &Value, mode: Mode, allowed: &[String]) -> PermissionAnswer {
    let action = data["action"].as_str().unwrap_or_default();
    let tool = allow_name(action);
    let summary = request_summary(data);
    let allow = allow_entry(&tool, &json!({ "command": summary }));
    let approved = permission::verdict(mode, action_kind(action)) == Verdict::Approve
        || allowed.iter().any(|entry| *entry == allow || *entry == tool);
    if approved {
        return PermissionAnswer::Approve;
    }
    let id = data["source"]["id"]
        .as_str()
        .or_else(|| data["id"].as_str())
        .unwrap_or_default()
        .to_owned();
    PermissionAnswer::Refuse(PermissionRequest { id, tool, summary, allow })
}

/// A tool's argument worth showing next to its name: the command, the file, the pattern.
fn input_summary(input: &Value) -> String {
    input
        .as_object()
        .and_then(|map| {
            [
                "command",
                "filePath",
                "file_path",
                "path",
                "pattern",
                "url",
                "query",
                "description",
            ]
            .iter()
            .find_map(|key| map.get(*key).and_then(Value::as_str))
        })
        .map(str::to_owned)
        .unwrap_or_else(|| input.to_string())
}

fn error_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
        Value::Object(map) => {
            for key in ["message", "detail", "reason", "error", "data", "name"] {
                if let Some(text) = map.get(key).and_then(error_text) {
                    return Some(text);
                }
            }
            None
        }
        _ => None,
    }
}

/// The output of `opencode models`: one `provider/model` per line, and an empty string when the CLI is missing,
/// not signed in, or does not answer in time (a background server may have to start first).
pub fn models_listing(binary: std::ffi::OsString) -> String {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut command = std::process::Command::new(binary);
        command
            .arg("models")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let listing = command
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default();
        let _ = sender.send(listing);
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(20))
        .unwrap_or_default()
}

pub struct OpenCode {
    session: Mutex<Option<String>>,
    /// Tool ids already announced, so a call that arrives already finished shows as one call, not a bare result.
    tools: Mutex<HashSet<String>>,
    /// The turn's answer text and cost so far, reported with the turn's end.
    answer: Mutex<Answer>,
    root: Mutex<Option<PathBuf>>,
    file_snapshots: Mutex<std::collections::HashMap<String, super::FileSnapshot>>,
}

impl Default for OpenCode {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            tools: Mutex::new(HashSet::new()),
            answer: Mutex::new(Answer::default()),
            root: Mutex::new(None),
            file_snapshots: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Provider for OpenCode {
    fn id(&self) -> &'static str {
        "opencode"
    }
    fn binary(&self) -> &'static str {
        "opencode"
    }
    fn candidates(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn prepare(&self, req: &TurnRequest) -> Result<(), String> {
        if let Ok(mut root) = self.root.lock() {
            *root = req.cwd.clone();
        }
        if let Ok(mut snapshots) = self.file_snapshots.lock() {
            snapshots.clear();
        }
        if let Ok(mut tools) = self.tools.lock() {
            tools.clear();
        }
        if let Ok(mut answer) = self.answer.lock() {
            *answer = Answer::default();
        }
        Ok(())
    }
    fn args(&self, req: &TurnRequest) -> Vec<String> {
        // `--thinking` puts the reasoning parts in the stream; a model without any simply sends none.
        // `--standalone` keeps the turn on its own server: the shared background service ignores the config handed
        // over in the environment (verified: a denied shell still ran) and is the user's own OpenCode session.
        let mut args = vec![
            "run".into(),
            "--standalone".into(),
            "--format".into(),
            "json".into(),
            "--thinking".into(),
        ];
        if let Some(model) = &req.model {
            args.extend(["--model".into(), model.clone()]);
        }
        // OpenCode session ids use `ses_…`. AgentDeck may still hold its own draft UUID when a run was stopped
        // before OpenCode returned an id, so never pass that unrelated id to the CLI's validated `--session` flag.
        if let Some(session) = req
            .session_id
            .as_ref()
            .filter(|session| req.resume && session.starts_with("ses_"))
        {
            args.extend(["--session".into(), session.clone()]);
        }
        // Only approving what the CLI's own rules allow maps to `--auto`; in every other mode OpenCode asks, and a
        // headless run that is asked stops at the call instead of hanging the turn.
        if super::permission::Mode::parse(req.permission_mode.as_deref()).opencode_auto() {
            args.push("--auto".into());
        }
        // Effort is not a flag here: OpenCode takes a model variant (`provider/model#variant`) and publishes no list
        // of them, so a chosen effort stays with whatever the model does by default.
        args
    }
    /// The prompt goes in on stdin: OpenCode reads it when no message argument is given, which keeps a Windows
    /// `.cmd` shim from re-parsing the text (a `&` or `|` in a message would otherwise start a second command) and
    /// takes the 32k command-line limit out of the way for a long paste.
    fn prompt_on_stdin(&self) -> bool {
        true
    }
    /// OpenCode has no system prompt flag here, so the app and ask instructions open each new session.
    fn opening_context(&self, req: &TurnRequest) -> Option<String> {
        let continues = req.resume && req.session_id.as_ref().is_some_and(|session| session.starts_with("ses_"));
        (!continues).then(|| format!("{APP_CONTEXT}{}", super::claude::ASK_CONTEXT))
    }
    fn environment(&self, req: &TurnRequest) -> Vec<(String, String)> {
        // OpenCode's own `question` tool waits for an answer that a headless run cannot give, so the turn hung. It is
        // denied here; the agent asks with the agentdeck-ask block instead (ASK_CONTEXT).
        let mut config = serde_json::json!({ "permission": { "question": "deny" } });
        if let Ok(bridge) = std::env::var("AGENTDECK_BRIDGE") {
            let url = super::mcp_url(&bridge, req.owner.as_deref(), req.cwd.as_deref());
            config["mcp"] = serde_json::json!({ "agentdeck": { "type": "remote", "url": url, "enabled": true } });
        }
        vec![("OPENCODE_CONFIG_CONTENT".into(), config.to_string())]
    }
    fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let session = value["sessionID"]
            .as_str()
            .or_else(|| value["session_id"].as_str())
            .map(str::to_owned);
        if let Some(id) = &session {
            if let Ok(mut current) = self.session.lock() {
                *current = Some(id.clone());
            }
        }
        let kind = value["type"].as_str().unwrap_or_default();
        let part = &value["part"];
        let text = value["text"]
            .as_str()
            .or_else(|| part["text"].as_str())
            .unwrap_or_default();
        match kind {
            "text" | "message" | "agent_message" => {
                if let Ok(mut answer) = self.answer.lock() {
                    answer.push_text(text);
                }
                (!text.is_empty())
                    .then_some(AgentEvent::Delta {
                        text: text.to_owned(),
                    })
                    .into_iter()
                    .collect()
            }
            "reasoning" | "thinking" => (!text.is_empty())
                .then_some(AgentEvent::Thinking {
                    text: text.to_owned(),
                })
                .into_iter()
                .collect(),
            "step_finish" => {
                if let Ok(mut answer) = self.answer.lock() {
                    answer.end_step(part["cost"].as_f64().unwrap_or_default());
                }
                let tokens = &part["tokens"];
                vec![AgentEvent::Usage {
                    input_tokens: tokens["input"].as_u64().unwrap_or(0),
                    output_tokens: tokens["output"].as_u64().unwrap_or(0)
                        + tokens["reasoning"].as_u64().unwrap_or(0),
                    cache_read_tokens: tokens["cache"]["read"].as_u64().unwrap_or(0),
                    cache_write_tokens: tokens["cache"]["write"].as_u64().unwrap_or(0),
                    new_call: true,
                }]
            }
            // One line per call, and its `state.status` says how far it got: a quick tool arrives already completed,
            // so the call is announced first and the result follows in the same step.
            "tool_use" => {
                let id = part["id"]
                    .as_str()
                    .or_else(|| part["callID"].as_str())
                    .unwrap_or("opencode-tool")
                    .to_owned();
                let settled = matches!(
                    part["state"]["status"].as_str(),
                    Some("completed" | "error")
                );
                let mut events = Vec::new();
                let first = self
                    .tools
                    .lock()
                    .map(|mut seen| seen.insert(id.clone()))
                    .unwrap_or(false);
                if first {
                    let tool = part["tool"].as_str().unwrap_or("tool");
                    let input = &part["state"]["input"];
                    let change = super::file_change(tool, input);
                    if change.is_none() {
                        let root = self.root.lock().ok().and_then(|root| root.clone());
                        if let Some(snapshot) = super::begin_file_snapshot(root.as_deref(), tool, input) {
                            if let Ok(mut snapshots) = self.file_snapshots.lock() {
                                snapshots.insert(id.clone(), snapshot);
                            }
                        }
                    }
                    events.push(AgentEvent::ToolCall {
                        id: id.clone(),
                        name: tool.to_owned(),
                        input: input_summary(input),
                        change,
                        background: tool == "subagent",
                    });
                    if tool == "subagent" {
                        events.push(subagent_task(part, &id, TaskStatus::Started));
                    }
                }
                if settled {
                    let is_error = part["state"]["status"] == "error";
                    let change = if is_error {
                        self.file_snapshots.lock().ok().and_then(|mut snapshots| snapshots.remove(&id));
                        None
                    } else {
                        self.file_snapshots
                            .lock()
                            .ok()
                            .and_then(|mut snapshots| snapshots.remove(&id))
                            .and_then(super::finish_file_snapshot)
                    };
                    events.push(AgentEvent::ToolResult {
                        id: id.clone(),
                        output: part["state"]["output"]
                            .as_str()
                            .or_else(|| part["state"]["error"].as_str())
                            .unwrap_or_default()
                            .to_owned(),
                        is_error,
                        image: None,
                        change,
                    });
                    if part["tool"] == "subagent" {
                        let status = if is_error { TaskStatus::Failed } else { TaskStatus::Done };
                        events.push(subagent_task(part, &id, status));
                    }
                }
                events
            }
            "done" | "session_end" => {
                let answer = self.answer.lock().map(|answer| answer.clone()).unwrap_or_default();
                vec![AgentEvent::Done {
                    session_id: session,
                    text: answer.text(),
                    cost_usd: value["cost"].as_f64().or(answer.cost_usd()),
                }]
            }
            "error" => vec![AgentEvent::Error {
                message: error_text(&value["error"])
                    .or_else(|| (!text.is_empty()).then_some(text.to_owned()))
                    .unwrap_or_else(|| "OpenCode failed".into()),
            }],
            _ => Vec::new(),
        }
    }
    /// `opencode run` ends without a result line: the last event of a good turn is its text, and the session id came
    /// with the first line, so the turn is finished here.
    fn finish(&self, _req: &TurnRequest) -> Option<AgentEvent> {
        let session_id = self.session.lock().ok().and_then(|mut id| id.take())?;
        let answer = self
            .answer
            .lock()
            .map(|mut answer| std::mem::take(&mut *answer))
            .unwrap_or_default();
        Some(AgentEvent::Done {
            session_id: Some(session_id),
            text: answer.text(),
            cost_usd: answer.cost_usd(),
        })
    }

    fn chat_capabilities(&self, _req: &TurnRequest) -> super::ChatResponseCapabilities {
        super::ChatResponseCapabilities::opencode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(prompt: &str) -> TurnRequest {
        TurnRequest {
            prompt: prompt.into(),
            session_id: None,
            resume: false,
            model: None,
            permission_mode: None,
            effort: None,
            allowed_tools: Vec::new(),
            cwd: None,
            owner: None,
            api_key: None,
        }
    }

    #[test]
    fn the_question_tool_is_denied_so_a_headless_turn_never_waits_for_an_answer() {
        let env = OpenCode::default().environment(&request("hi"));
        let config: Value = serde_json::from_str(&env[0].1).expect("config json");
        assert_eq!(config["permission"]["question"], "deny");
    }

    #[test]
    fn the_opening_context_goes_only_to_a_new_session() {
        let mut fresh = request("hi");
        assert!(OpenCode::default().opening_context(&fresh).is_some_and(|text| text.contains("```agentdeck-ask")));
        fresh.session_id = Some("ses_1".into());
        fresh.resume = true;
        assert!(OpenCode::default().opening_context(&fresh).is_none());
        fresh.session_id = Some("draft-uuid".into());
        assert!(OpenCode::default().opening_context(&fresh).is_some());
    }

    #[test]
    fn parses_text_usage_tools_and_finish() {
        let provider = OpenCode::default();
        assert_eq!(
            provider.parse_line(r#"{"type":"text","sessionID":"ses_1","part":{"text":"hi"}}"#),
            vec![AgentEvent::Delta { text: "hi".into() }]
        );
        assert!(
            matches!(&provider.parse_line(r#"{"type":"tool_use","part":{"callID":"c1","tool":"bash","state":{"status":"running","input":{"command":"pwd"}}}}"#)[0], AgentEvent::ToolCall { name, .. } if name == "bash")
        );
        assert!(
            matches!(provider.finish(&request("")), Some(AgentEvent::Done { session_id: Some(id), .. }) if id == "ses_1")
        );
    }

    /// What the CLI really writes: a fast tool is already finished when its line arrives.
    #[test]
    fn a_finished_tool_line_is_a_call_and_a_result() {
        let provider = OpenCode::default();
        let line = r#"{"type":"tool_use","sessionID":"ses_2","part":{"id":"call_1","tool":"shell","state":{"status":"completed","input":{"command":"echo hi"},"output":"hi\n"}}}"#;
        assert_eq!(
            provider.parse_line(line),
            vec![
                AgentEvent::ToolCall {
                    id: "call_1".into(),
                    name: "shell".into(),
                    input: "echo hi".into(),
                    change: None,
                    background: false
                },
                AgentEvent::ToolResult {
                    id: "call_1".into(),
                    output: "hi\n".into(),
                    is_error: false,
                    image: None,
                    change: None,
                },
            ]
        );
        // The same call reported again only carries what changed: its result.
        let done = r#"{"type":"tool_use","part":{"id":"call_1","tool":"shell","state":{"status":"error","input":{"command":"echo hi"},"error":"nope"}}}"#;
        assert_eq!(
            provider.parse_line(done),
            vec![AgentEvent::ToolResult {
                id: "call_1".into(),
                output: "nope".into(),
                is_error: true,
                image: None,
                change: None
            }]
        );
    }

    /// A `subagent` tool line as the CLI writes it (live-opencode-5-subagent): a call, a background task and its end.
    #[test]
    fn a_subagent_call_is_a_background_task_with_the_child_session() {
        let provider = OpenCode::default();
        let line = r#"{"type":"tool_use","sessionID":"ses_main","part":{"partID":"prt_1","id":"chatcmpl-tool-9","tool":"subagent","state":{"status":"completed","input":{"agent":"general","description":"Compute 2+2","prompt":"compute"},"output":"<subagent sessionID=\"ses_child\" state=\"completed\">\n4\n</subagent>","metadata":{"metadata":{"sessionID":"ses_child","status":"completed"}}}}}"#;
        let events = provider.parse_line(line);
        assert!(matches!(&events[0], AgentEvent::ToolCall { name, background: true, .. } if name == "subagent"));
        let task = |status| AgentEvent::Task {
            id: "ses_child".into(),
            tool_use_id: Some("chatcmpl-tool-9".into()),
            status,
            label: Some("general: Compute 2+2".into()),
            summary: None,
            tools: None,
        };
        assert_eq!(events[1], task(TaskStatus::Started));
        assert!(matches!(&events[2], AgentEvent::ToolResult { is_error: false, .. }));
        assert_eq!(events[3], task(TaskStatus::Done));
        assert_eq!(events.len(), 4);
    }

    #[test]
    fn a_failed_subagent_call_ends_its_task_as_failed() {
        let provider = OpenCode::default();
        let line = r#"{"type":"tool_use","part":{"id":"c9","tool":"subagent","state":{"status":"error","input":{"agent":"general"},"error":"boom"}}}"#;
        let events = provider.parse_line(line);
        assert!(matches!(events.last(), Some(AgentEvent::Task { status: TaskStatus::Failed, summary: Some(text), id, .. }) if text == "boom" && id == "c9"));
    }

    #[test]
    fn a_running_tool_line_waits_for_its_result() {
        let provider = OpenCode::default();
        let running = r#"{"type":"tool_use","part":{"id":"call_2","tool":"edit","state":{"status":"running","input":{"filePath":"a.ts"}}}}"#;
        assert_eq!(
            provider.parse_line(running),
            vec![AgentEvent::ToolCall {
                id: "call_2".into(),
                name: "edit".into(),
                input: "a.ts".into(),
                change: None,
                background: false
            }]
        );
        let finished = r#"{"type":"tool_use","part":{"id":"call_2","tool":"edit","state":{"status":"completed","input":{"filePath":"a.ts"},"output":"ok"}}}"#;
        assert_eq!(
            provider.parse_line(finished),
            vec![AgentEvent::ToolResult {
                id: "call_2".into(),
                output: "ok".into(),
                is_error: false,
                image: None,
                change: None
            }]
        );
    }

    #[test]
    fn parses_error_shapes_from_opencode() {
        let provider = OpenCode::default();
        assert_eq!(
            provider.parse_line(
                r#"{"type":"error","error":{"type":"provider.auth","message":"Not logged in"}}"#
            ),
            vec![AgentEvent::Error {
                message: "Not logged in".into()
            }]
        );
        assert_eq!(
            provider.parse_line(
                r#"{"type":"error","error":{"data":{"message":"Provider unavailable"}}}"#
            ),
            vec![AgentEvent::Error {
                message: "Provider unavailable".into()
            }]
        );
    }

    #[test]
    fn args_carry_the_model_the_session_and_auto_only_when_asking_is_off() {
        let provider = OpenCode::default();
        let mut req = request("say hi");
        assert_eq!(
            provider.args(&req),
            vec!["run", "--standalone", "--format", "json", "--thinking"]
        );
        // The prompt is not an argument at all: it is written to stdin, so shell characters in it stay text.
        assert!(!provider.args(&req).iter().any(|arg| arg.contains("say hi")));
        assert!(provider.prompt_on_stdin());

        req.session_id = Some("ses_9".into());
        req.model = Some("opencode/big-pickle".into());
        // A session id is only continued when the caller resumes it; a new chat that falls back to the old id must
        // not silently continue the old conversation.
        assert_eq!(
            provider.args(&req),
            vec![
                "run",
                "--standalone",
                "--format",
                "json",
                "--thinking",
                "--model",
                "opencode/big-pickle"
            ]
        );
        req.resume = true;
        let args = provider.args(&req);
        assert_eq!(
            (
                args.contains(&"--session".to_string()),
                args.contains(&"ses_9".to_string())
            ),
            (true, true)
        );

        req.permission_mode = Some("ask".into());
        assert!(!provider.args(&req).contains(&"--auto".to_string()));
        req.permission_mode = Some("auto".into());
        assert!(provider.args(&req).contains(&"--auto".to_string()));
    }

    fn request_of(action: &str, metadata: Value) -> Value {
        json!({ "id": "per_1", "sessionID": "ses_1", "action": action, "metadata": metadata, "source": { "type": "tool", "messageID": "msg_1", "id": "call_7" } })
    }

    #[test]
    fn a_refused_command_is_offered_as_a_card_that_allows_exactly_it() {
        let shell = request_of("shell", json!({ "command": "git push" }));
        let answer = answer_permission(&shell, Mode::Ask, &[]);
        assert_eq!(
            answer,
            PermissionAnswer::Refuse(PermissionRequest {
                id: "call_7".into(),
                tool: "Bash".into(),
                summary: "git push".into(),
                allow: "Bash(git push)".into(),
            })
        );
        // The next turn carries that entry, and then the same command runs.
        assert_eq!(answer_permission(&shell, Mode::Ask, &["Bash(git push)".into()]), PermissionAnswer::Approve);
        // Another command is still refused, and a bare `Bash` approves every command.
        let other = request_of("shell", json!({ "command": "rm -rf x" }));
        assert!(matches!(answer_permission(&other, Mode::Ask, &["Bash(git push)".into()]), PermissionAnswer::Refuse(_)));
        assert_eq!(answer_permission(&other, Mode::Ask, &["Bash".into()]), PermissionAnswer::Approve);
    }

    #[test]
    fn the_modes_decide_what_runs_without_asking() {
        let edit = request_of("edit", json!({ "filepath": "a.ts" }));
        let shell = request_of("shell", json!({ "command": "ls" }));
        let read = request_of("read", json!({}));
        assert_eq!(answer_permission(&edit, Mode::Edits, &[]), PermissionAnswer::Approve);
        assert!(matches!(answer_permission(&shell, Mode::Edits, &[]), PermissionAnswer::Refuse(_)));
        assert_eq!(answer_permission(&shell, Mode::Auto, &[]), PermissionAnswer::Approve);
        assert_eq!(answer_permission(&shell, Mode::Yolo, &[]), PermissionAnswer::Approve);
        assert_eq!(answer_permission(&read, Mode::Plan, &[]), PermissionAnswer::Approve);
        // Plan refuses changes and commands; the card names the file it would have changed.
        match answer_permission(&edit, Mode::Plan, &[]) {
            PermissionAnswer::Refuse(request) => assert_eq!((request.tool.as_str(), request.summary.as_str()), ("Write", "a.ts")),
            other => panic!("plan must refuse an edit: {other:?}"),
        }
    }

    #[test]
    fn a_request_without_metadata_is_summarised_by_its_resources_and_id_falls_back() {
        let data = json!({ "id": "per_9", "sessionID": "ses_1", "action": "external_directory", "resources": ["/tmp/a", "/tmp/b"] });
        match answer_permission(&data, Mode::Ask, &[]) {
            PermissionAnswer::Refuse(request) => {
                assert_eq!(request.summary, "/tmp/a, /tmp/b");
                assert_eq!(request.id, "per_9");
                assert_eq!(request.allow, "external_directory");
            }
            other => panic!("expected a refusal: {other:?}"),
        }
    }

    #[test]
    fn the_answer_is_the_last_step_that_wrote_text_and_the_cost_adds_up() {
        let mut answer = Answer::default();
        assert_eq!((answer.text(), answer.cost_usd()), (String::new(), None));
        answer.push_text("I'll launch three subagents.");
        answer.end_step(0.0);
        answer.push_text("4,6,8");
        answer.end_step(0.25);
        // A last step that writes nothing keeps the text before it.
        answer.end_step(0.0);
        assert_eq!(answer.text(), "4,6,8");
        assert_eq!(answer.cost_usd(), Some(0.25));
    }

    #[test]
    fn a_subagent_card_is_keyed_by_its_call_and_labelled_by_agent_and_description() {
        let input = json!({ "agent": "general", "description": "Compute 2+2" });
        assert_eq!(
            subagent_event(&input, "call_3", TaskStatus::Started, None),
            AgentEvent::Task {
                id: "call_3".into(),
                tool_use_id: Some("call_3".into()),
                status: TaskStatus::Started,
                label: Some("general: Compute 2+2".into()),
                summary: None,
                tools: None,
            }
        );
        assert!(is_subagent("subagent") && is_subagent("task") && !is_subagent("bash"));
    }

    /// Finished text, a tool call in between and the step's end: the Done carries the final answer, as a Claude
    /// `result` does, so a delegated report has what it needs.
    #[test]
    fn the_done_event_carries_the_final_answer_text() {
        let provider = OpenCode::default();
        provider.parse_line(r#"{"type":"text","sessionID":"ses_3","part":{"text":"Let me check."}}"#);
        provider.parse_line(r#"{"type":"step_finish","sessionID":"ses_3","part":{"cost":0.1,"tokens":{"input":1,"output":1}}}"#);
        provider.parse_line(r#"{"type":"text","sessionID":"ses_3","part":{"text":"Done: 4"}}"#);
        assert_eq!(
            provider.finish(&request("")),
            Some(AgentEvent::Done {
                session_id: Some("ses_3".into()),
                text: "Done: 4".into(),
                cost_usd: Some(0.1),
            })
        );
    }
}
