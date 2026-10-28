//! OpenCode, driven headless with `opencode run --format json`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

use super::{AgentEvent, Provider, TurnRequest};

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
    /// Sum of the steps' `cost`, reported with the turn's end.
    cost: Mutex<f64>,
    root: Mutex<Option<PathBuf>>,
    file_snapshots: Mutex<std::collections::HashMap<String, super::FileSnapshot>>,
}

impl Default for OpenCode {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            tools: Mutex::new(HashSet::new()),
            cost: Mutex::new(0.0),
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
    fn environment(&self, req: &TurnRequest) -> Vec<(String, String)> {
        let Ok(bridge) = std::env::var("AGENTDECK_BRIDGE") else {
            return Vec::new();
        };
        let url = super::mcp_url(&bridge, req.owner.as_deref(), req.cwd.as_deref());
        vec![(
            "OPENCODE_CONFIG_CONTENT".into(),
            serde_json::json!({
                "mcp": { "agentdeck": { "type": "remote", "url": url, "enabled": true } }
            })
            .to_string(),
        )]
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
            "text" | "message" | "agent_message" => (!text.is_empty())
                .then_some(AgentEvent::Delta {
                    text: text.to_owned(),
                })
                .into_iter()
                .collect(),
            "reasoning" | "thinking" => (!text.is_empty())
                .then_some(AgentEvent::Thinking {
                    text: text.to_owned(),
                })
                .into_iter()
                .collect(),
            "step_finish" => {
                if let Some(cost) = part["cost"].as_f64() {
                    if let Ok(mut total) = self.cost.lock() {
                        *total += cost;
                    }
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
                        background: false,
                    });
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
                        id,
                        output: part["state"]["output"]
                            .as_str()
                            .or_else(|| part["state"]["error"].as_str())
                            .unwrap_or_default()
                            .to_owned(),
                        is_error,
                        image: None,
                        change,
                    });
                }
                events
            }
            "done" | "session_end" => vec![AgentEvent::Done {
                session_id: session,
                text: String::new(),
                cost_usd: value["cost"].as_f64(),
            }],
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
        let cost = self
            .cost
            .lock()
            .map(|mut total| std::mem::take(&mut *total))
            .unwrap_or(0.0);
        Some(AgentEvent::Done {
            session_id: Some(session_id),
            text: String::new(),
            cost_usd: (cost > 0.0).then_some(cost),
        })
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
}
