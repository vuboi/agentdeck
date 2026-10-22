//! Claude Code, driven headless: `claude -p <prompt> --output-format stream-json`.
//!
//! One process per turn. Context across turns comes from the conversation id: the first turn passes
//! `--session-id <uuid>`, later turns pass `--resume <uuid>`.

use std::path::PathBuf;

use serde_json::Value;

use super::{
    allow_entry, file_change, summarize_tool_input, AgentEvent, PermissionRequest, Provider,
    TurnRequest,
};

pub struct Claude;

/// Added to the CLI's system prompt on every turn.
const APP_CONTEXT: &str = "You are running inside AgentDeck, a desktop app that has its own built-in browser (the Preview panel). When the user asks you to open, preview or show a web page or a local dev server, do not open it with an external browser or any browser-automation tool. Start the server if it is needed, then write the full address (for example http://localhost:4200) in your reply: AgentDeck opens that address in its Preview panel by itself.";

/// Text of a tool result: a plain string, or an array of `{ "type": "text", "text": … }`.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Where the Claude Code CLI usually lives, per platform. A desktop app started from the Dock, the Start
/// menu or a launcher does not get the shell's PATH, so the usual install locations are tried first.
fn candidates_for(os: &str, home: Option<PathBuf>, appdata: Option<PathBuf>) -> Vec<PathBuf> {
    let exe = if os == "windows" {
        "claude.exe"
    } else {
        "claude"
    };
    let mut found = Vec::new();
    if let Some(home) = &home {
        // The native installer, then the older self-updating install.
        found.push(home.join(".local").join("bin").join(exe));
        found.push(home.join(".claude").join("local").join(exe));
    }
    match os {
        "macos" => {
            found.push(PathBuf::from("/opt/homebrew/bin/claude"));
            found.push(PathBuf::from("/usr/local/bin/claude"));
        }
        "windows" => {
            // npm's global bin is %APPDATA%\npm, where the CLI is a .cmd shim.
            if let Some(appdata) = &appdata {
                found.push(appdata.join("npm").join("claude.cmd"));
            }
        }
        _ => {
            found.push(PathBuf::from("/usr/local/bin/claude"));
            found.push(PathBuf::from("/usr/bin/claude"));
            if let Some(home) = &home {
                found.push(home.join(".npm-global").join("bin").join("claude"));
            }
        }
    }
    found
}

impl Provider for Claude {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn binary(&self) -> &'static str {
        "claude"
    }

    fn candidates(&self) -> Vec<PathBuf> {
        let env_path = |key: &str| std::env::var_os(key).map(PathBuf::from);
        let home = env_path("HOME").or_else(|| env_path("USERPROFILE"));
        candidates_for(std::env::consts::OS, home, env_path("APPDATA"))
    }

    fn args(&self, req: &TurnRequest) -> Vec<String> {
        let mut args = vec![
            "-p".to_string(),
            req.prompt.clone(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            // Token-level deltas, so text appears while it is written.
            "--include-partial-messages".to_string(),
        ];
        if let Some(id) = &req.session_id {
            args.push(
                if req.resume {
                    "--resume"
                } else {
                    "--session-id"
                }
                .to_string(),
            );
            args.push(id.clone());
        }
        if let Some(model) = &req.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }
        // AgentDeck has its own browser; the agent is told to hand pages over to it instead of opening one itself.
        args.push("--append-system-prompt".to_string());
        args.push(APP_CONTEXT.to_string());
        if let Some(effort) = &req.effort {
            args.push("--effort".to_string());
            args.push(effort.clone());
        }
        if let Some(mode) = &req.permission_mode {
            args.push("--permission-mode".to_string());
            args.push(mode.clone());
        }
        for tool in &req.allowed_tools {
            args.push("--allowedTools".to_string());
            args.push(tool.clone());
        }
        args
    }

    fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match value["type"].as_str() {
            // Partial message chunks (needs --include-partial-messages).
            Some("stream_event") => {
                let delta = &value["event"]["delta"];
                match (value["event"]["type"].as_str(), delta["type"].as_str()) {
                    (Some("content_block_delta"), Some("text_delta")) => {
                        vec![AgentEvent::Delta {
                            text: delta["text"].as_str().unwrap_or_default().to_string(),
                        }]
                    }
                    (Some("content_block_delta"), Some("thinking_delta")) => {
                        vec![AgentEvent::Thinking {
                            text: delta["thinking"].as_str().unwrap_or_default().to_string(),
                        }]
                    }
                    _ => Vec::new(),
                }
            }
            // The full assistant message. Text is already streamed above, so only tool calls are taken here.
            Some("assistant") => value["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|block| block["type"] == "tool_use")
                .map(|block| {
                    let name = block["name"].as_str().unwrap_or_default();
                    AgentEvent::ToolCall {
                        id: block["id"].as_str().unwrap_or_default().to_string(),
                        name: name.to_string(),
                        input: summarize_tool_input(name, &block["input"]),
                        change: file_change(name, &block["input"]),
                    }
                })
                .collect(),
            // Tool output comes back as a "user" message made of tool_result blocks.
            Some("user") => value["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|block| block["type"] == "tool_result")
                .map(|block| AgentEvent::ToolResult {
                    id: block["tool_use_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    output: result_text(&block["content"]),
                    is_error: block["is_error"].as_bool().unwrap_or(false),
                })
                .collect(),
            Some("result") => {
                let denied: Vec<PermissionRequest> = value["permission_denials"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|denial| {
                        let tool = denial["tool_name"].as_str().unwrap_or_default();
                        PermissionRequest {
                            id: denial["tool_use_id"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            tool: tool.to_string(),
                            summary: summarize_tool_input(tool, &denial["tool_input"]),
                            allow: allow_entry(tool, &denial["tool_input"]),
                        }
                    })
                    .collect();
                let mut events = Vec::new();
                if !denied.is_empty() {
                    events.push(AgentEvent::PermissionDenied { requests: denied });
                }
                if value["is_error"].as_bool().unwrap_or(false) {
                    events.push(AgentEvent::Error {
                        message: value["result"]
                            .as_str()
                            .unwrap_or("agent failed")
                            .to_string(),
                    });
                } else {
                    events.push(AgentEvent::Done {
                        session_id: value["session_id"].as_str().map(str::to_string),
                        text: value["result"].as_str().unwrap_or_default().to_string(),
                        cost_usd: value["total_cost_usd"].as_f64(),
                    });
                }
                events
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(resume: bool) -> TurnRequest {
        TurnRequest {
            prompt: "hi".into(),
            session_id: Some("abc".into()),
            resume,
            model: None,
            permission_mode: None,
            effort: None,
            allowed_tools: Vec::new(),
            cwd: None,
        }
    }

    #[test]
    fn first_turn_creates_the_session_and_later_turns_resume_it() {
        assert!(Claude
            .args(&req(false))
            .windows(2)
            .any(|w| w == ["--session-id", "abc"]));
        assert!(Claude
            .args(&req(true))
            .windows(2)
            .any(|w| w == ["--resume", "abc"]));
    }

    #[test]
    fn text_deltas_stream_as_delta_events() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"po"}}}"#;
        assert_eq!(
            Claude.parse_line(line),
            vec![AgentEvent::Delta { text: "po".into() }]
        );
    }

    #[test]
    fn result_line_finishes_the_turn_with_the_session_id() {
        let line = r#"{"type":"result","subtype":"success","is_error":false,"result":"pong","session_id":"s1","total_cost_usd":0.5}"#;
        assert_eq!(
            Claude.parse_line(line),
            vec![AgentEvent::Done {
                session_id: Some("s1".into()),
                text: "pong".into(),
                cost_usd: Some(0.5)
            }]
        );
    }

    #[test]
    fn error_result_becomes_an_error_event() {
        let line = r#"{"type":"result","is_error":true,"result":"rate limited"}"#;
        assert_eq!(
            Claude.parse_line(line),
            vec![AgentEvent::Error {
                message: "rate limited".into()
            }]
        );
    }

    #[test]
    fn tool_calls_and_their_output_are_linked_by_id() {
        let call = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls -la"}}]}}"#;
        assert_eq!(
            Claude.parse_line(call),
            vec![AgentEvent::ToolCall {
                id: "t1".into(),
                name: "Bash".into(),
                input: "ls -la".into(),
                change: None
            }]
        );
    }

    #[test]
    fn writes_and_edits_report_what_they_change() {
        let write = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"w","name":"Write","input":{"file_path":"a.txt","content":"x\ny\nz"}}]}}"#;
        let events = Claude.parse_line(write);
        let AgentEvent::ToolCall {
            change: Some(change),
            ..
        } = &events[0]
        else {
            panic!("no change: {events:?}")
        };
        assert_eq!(
            (change.path.as_str(), change.added, change.removed),
            ("a.txt", 3, 0)
        );

        let edit = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"e","name":"Edit","input":{"file_path":"a.txt","old_string":"x","new_string":"p\nq"}}]}}"#;
        let events = Claude.parse_line(edit);
        let AgentEvent::ToolCall {
            change: Some(change),
            ..
        } = &events[0]
        else {
            panic!("no change: {events:?}")
        };
        assert_eq!((change.added, change.removed), (2, 1));
        let result = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"a\nb"}],"is_error":true}]}}"#;
        assert_eq!(
            Claude.parse_line(result),
            vec![AgentEvent::ToolResult {
                id: "t1".into(),
                output: "a\nb".into(),
                is_error: true
            }]
        );
    }

    #[test]
    fn refused_tools_become_a_permission_request_before_done() {
        let line = r#"{"type":"result","is_error":false,"result":"blocked","session_id":"s1","permission_denials":[{"tool_name":"Bash","tool_use_id":"t9","tool_input":{"command":"git push"}}]}"#;
        let events = Claude.parse_line(line);
        assert_eq!(
            events[0],
            AgentEvent::PermissionDenied {
                requests: vec![PermissionRequest {
                    id: "t9".into(),
                    tool: "Bash".into(),
                    summary: "git push".into(),
                    allow: "Bash(git push)".into()
                }]
            }
        );
        assert!(matches!(events[1], AgentEvent::Done { .. }));
    }

    #[test]
    fn the_agent_is_told_about_the_built_in_browser() {
        let args = Claude.args(&req(false));
        let at = args
            .iter()
            .position(|arg| arg == "--append-system-prompt")
            .unwrap();
        assert!(args[at + 1].contains("Preview panel"));
    }

    #[test]
    fn permission_mode_and_approved_tools_reach_the_cli() {
        let mut request = req(true);
        request.permission_mode = Some("acceptEdits".into());
        request.allowed_tools = vec!["Bash(ls)".into(), "Write".into()];
        let args = Claude.args(&request);
        assert!(args
            .windows(2)
            .any(|w| w == ["--permission-mode", "acceptEdits"]));
        assert!(args.windows(2).any(|w| w == ["--allowedTools", "Bash(ls)"]));
        assert!(args.windows(2).any(|w| w == ["--allowedTools", "Write"]));
    }

    #[test]
    fn unknown_lines_are_ignored() {
        assert!(Claude
            .parse_line(r#"{"type":"system","subtype":"init"}"#)
            .is_empty());
        assert!(Claude.parse_line("not json").is_empty());
    }
}

#[cfg(test)]
mod platform_tests {
    use super::*;

    fn home() -> Option<PathBuf> {
        Some(PathBuf::from("/h"))
    }

    #[test]
    fn each_platform_gets_its_own_install_locations() {
        let win = candidates_for(
            "windows",
            Some(PathBuf::from("C:/u")),
            Some(PathBuf::from("C:/a")),
        );
        assert!(win[0].ends_with("claude.exe") && win.iter().any(|p| p.ends_with("claude.cmd")));
        let mac = candidates_for("macos", home(), None);
        assert!(mac.contains(&PathBuf::from("/opt/homebrew/bin/claude")));
        assert!(mac[0].ends_with("claude") && !mac[0].to_string_lossy().ends_with(".exe"));
        let linux = candidates_for("linux", home(), None);
        assert!(linux.contains(&PathBuf::from("/usr/bin/claude")));
    }
}
