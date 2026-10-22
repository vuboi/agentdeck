//! Codex, driven headless: `codex exec --json <prompt>`, one process per turn.
//!
//! Context across turns comes from the thread id the first turn reports (`thread.started`); later turns run
//! `codex exec resume <id>`. Options go through `-c key=value`, which both `exec` and `exec resume` accept.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

use super::{AgentEvent, FileChange, Provider, TurnRequest};

/// Added to Codex's instructions on every turn (the same facts Claude is given).
const APP_CONTEXT: &str = "You are running inside AgentDeck, a desktop app with its own built-in browser (the Preview panel). To show a web page or a local dev server, start the server if needed and write the full address of the server you actually started in your reply: AgentDeck opens it in Preview. Before starting a dev server, inspect the workspace scripts and identify the requested framework. In the z-ui workspace, use `pnpm start:svelte` for the Svelte 5 app (default http://localhost:4310) and `pnpm start:angular` for the Angular app (default http://localhost:4200); never use the Angular 4200 server when the user asks for Svelte, Svelte 5, or z-ui-svelte. When the user asks you to inspect or interact with that page, you MUST use the AgentDeck MCP tools on the Preview page: call browser_screenshot first, then browser_click/browser_type/browser_key/browser_scroll as needed, and call browser_screenshot or browser_dom again to verify the result. Do not use an external Chrome tab, do not merely navigate to or mention the URL, and do not claim an interaction happened without a matching MCP result. You can also use browser_eval, browser_console, and browser_network for inspection. Never tell the user you cannot test in Preview.";

/// What one run of the parser remembers between lines: the thread id for the final event, which commands
/// were already announced, and whether the last thing written was answer text.
#[derive(Default)]
struct Turn {
    thread: Option<String>,
    started: HashSet<String>,
    last_was_text: bool,
}

#[derive(Default)]
pub struct Codex {
    turn: Mutex<Turn>,
}

/// TOML string for a `-c key=value` override.
fn toml_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}

/// Where the native Codex binary lives. The npm and pnpm launchers are scripts around it; starting the binary
/// itself keeps arguments (newlines, quotes) intact on Windows, where batch files mangle them.
fn candidates_for(os: &str, home: Option<PathBuf>, local: Option<PathBuf>, appdata: Option<PathBuf>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if os == "windows" {
        let triple = "x86_64-pc-windows-msvc";
        if let Some(local) = &local {
            // pnpm: %LOCALAPPDATA%\pnpm\global\<n>\.pnpm\@openai+codex@<version>-win32-x64\node_modules\@openai\codex\vendor\…
            let store = local.join("pnpm").join("global");
            for global in std::fs::read_dir(&store).into_iter().flatten().flatten() {
                let pnpm = global.path().join(".pnpm");
                let mut versions: Vec<PathBuf> = std::fs::read_dir(&pnpm)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with("@openai+codex@") && name.ends_with("-win32-x64"))
                    })
                    .collect();
                versions.sort();
                for version in versions.into_iter().rev() {
                    let vendor = version.join("node_modules").join("@openai").join("codex").join("vendor").join(triple);
                    found.push(vendor.join("bin").join("codex.exe"));
                    found.push(vendor.join("codex").join("codex.exe"));
                }
            }
        }
        if let Some(appdata) = &appdata {
            let vendor = appdata.join("npm").join("node_modules").join("@openai").join("codex").join("vendor").join(triple);
            found.push(vendor.join("bin").join("codex.exe"));
            found.push(vendor.join("codex").join("codex.exe"));
        }
    } else {
        if let Some(home) = &home {
            found.push(home.join(".local").join("bin").join("codex"));
        }
        found.push(PathBuf::from("/opt/homebrew/bin/codex"));
        found.push(PathBuf::from("/usr/local/bin/codex"));
        found.push(PathBuf::from("/usr/bin/codex"));
    }
    found
}

/// A shell command as Codex reports it, without the shell wrapper around it.
fn bare_command(command: &str) -> String {
    for marker in ["-Command \"", "-Command '", "-lc '", "-lc \"", "-c \""] {
        if let Some(start) = command.find(marker) {
            let inner = &command[start + marker.len()..];
            return inner.trim_end_matches(['"', '\'']).to_string();
        }
    }
    command.to_string()
}

impl Provider for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn binary(&self) -> &'static str {
        "codex"
    }

    fn candidates(&self) -> Vec<PathBuf> {
        let env_path = |key: &str| std::env::var_os(key).map(PathBuf::from);
        let home = env_path("HOME").or_else(|| env_path("USERPROFILE"));
        candidates_for(std::env::consts::OS, home, env_path("LOCALAPPDATA"), env_path("APPDATA"))
    }

    fn args(&self, req: &TurnRequest) -> Vec<String> {
        if let Ok(mut turn) = self.turn.lock() {
            *turn = Turn::default();
        }
        let mut args = vec!["exec".to_string()];
        if req.permission_mode.as_deref() == Some("auto") {
            args.push("--approve-for-me".to_string());
        }
        // Codex names its own threads, so only a thread it reported before can be resumed.
        if let (Some(id), true) = (&req.session_id, req.resume) {
            args.push("resume".to_string());
            args.push(id.clone());
        }
        args.push("--json".to_string());
        args.push("--skip-git-repo-check".to_string());
        if let Some(model) = &req.model {
            args.push("-m".to_string());
            args.push(model.clone());
        }
        let mut config = |key: &str, value: String| {
            args.push("-c".to_string());
            args.push(format!("{key}={value}"));
        };
        if let Some(effort) = &req.effort {
            config("model_reasoning_effort", toml_string(effort));
        }
        // Codex cannot stop to ask in exec mode: the approval modes map to how far its sandbox reaches.
        let sandbox = match req.permission_mode.as_deref() {
            Some("acceptEdits") | Some("auto") => Some("workspace-write"),
            Some("bypassPermissions") => None,
            _ => Some("read-only"),
        };
        if let Some(mode) = sandbox {
            config("sandbox_mode", toml_string(mode));
            if req.permission_mode.as_deref() != Some("auto") {
                config("approval_policy", toml_string("never"));
            }
        }
        config("developer_instructions", toml_string(APP_CONTEXT));
        if let Ok(bridge) = std::env::var("AGENTDECK_BRIDGE") {
            config("mcp_servers.agentdeck.url", toml_string(&format!("{bridge}/mcp")));
        }
        if sandbox.is_none() {
            args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
        }
        args.push(req.prompt.clone());
        args
    }

    fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let Ok(mut turn) = self.turn.lock() else {
            return Vec::new();
        };
        let item = &value["item"];
        let id = item["id"].as_str().unwrap_or_default().to_string();
        let kind = item["type"].as_str().unwrap_or_default();
        let mut events = Vec::new();
        match (value["type"].as_str().unwrap_or_default(), kind) {
            ("thread.started", _) => turn.thread = value["thread_id"].as_str().map(str::to_owned),
            ("item.started", "command_execution") => {
                turn.started.insert(id.clone());
                turn.last_was_text = false;
                events.push(AgentEvent::ToolCall {
                    id,
                    name: "Bash".into(),
                    input: bare_command(item["command"].as_str().unwrap_or_default()),
                    change: None,
                    background: false,
                });
            }
            ("item.completed", "command_execution") => {
                if turn.started.insert(id.clone()) {
                    events.push(AgentEvent::ToolCall {
                        id: id.clone(),
                        name: "Bash".into(),
                        input: bare_command(item["command"].as_str().unwrap_or_default()),
                        change: None,
                        background: false,
                    });
                }
                let failed = item["exit_code"].as_i64().is_some_and(|code| code != 0) || item["status"] == "failed";
                events.push(AgentEvent::ToolResult {
                    id,
                    output: item["aggregated_output"].as_str().unwrap_or_default().to_string(),
                    is_error: failed,
                });
                turn.last_was_text = false;
            }
            ("item.completed", "agent_message") => {
                let text = item["text"].as_str().unwrap_or_default();
                if !text.is_empty() {
                    // Two messages in a row are two paragraphs, not one run-on line.
                    let joined = if turn.last_was_text { format!("\n\n{text}") } else { text.to_string() };
                    events.push(AgentEvent::Delta { text: joined });
                    turn.last_was_text = true;
                }
            }
            ("item.completed", "reasoning") => {
                let text = item["text"].as_str().unwrap_or_default();
                if !text.is_empty() {
                    events.push(AgentEvent::Thinking { text: text.to_string() });
                    turn.last_was_text = false;
                }
            }
            ("item.completed", "file_change") => {
                let changes = item["changes"].as_array().cloned().unwrap_or_default();
                for (index, change) in changes.iter().enumerate() {
                    let path = change["path"].as_str().unwrap_or_default().to_string();
                    let call = format!("{id}:{index}");
                    let name = if change["kind"] == "add" { "Write" } else { "Edit" };
                    events.push(AgentEvent::ToolCall {
                        id: call.clone(),
                        name: name.into(),
                        input: path.clone(),
                        change: Some(FileChange { path, added: 0, removed: 0, before: String::new(), after: String::new() }),
                        background: false,
                    });
                    events.push(AgentEvent::ToolResult { id: call, output: String::new(), is_error: item["status"] == "failed" });
                }
                turn.last_was_text = false;
            }
            ("item.started", "mcp_tool_call") | ("item.completed", "mcp_tool_call") => {
                let name = format!("{}.{}", item["server"].as_str().unwrap_or("mcp"), item["tool"].as_str().unwrap_or_default());
                if turn.started.insert(id.clone()) {
                    events.push(AgentEvent::ToolCall {
                        id: id.clone(),
                        name,
                        input: item["arguments"].to_string().chars().take(200).collect(),
                        change: None,
                        background: false,
                    });
                }
                if value["type"] == "item.completed" {
                    let output = item["result"]["content"]
                        .as_array()
                        .map(|parts| parts.iter().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join("\n"))
                        .unwrap_or_default();
                    events.push(AgentEvent::ToolResult {
                        id,
                        output: if output.is_empty() { item["error"]["message"].as_str().unwrap_or_default().to_string() } else { output },
                        is_error: item["status"] == "failed",
                    });
                }
                turn.last_was_text = false;
            }
            ("item.completed", "web_search") => {
                events.push(AgentEvent::ToolCall {
                    id: id.clone(),
                    name: "WebSearch".into(),
                    input: item["query"].as_str().unwrap_or_default().to_string(),
                    change: None,
                    background: false,
                });
                events.push(AgentEvent::ToolResult { id, output: String::new(), is_error: false });
            }
            ("item.completed", "error") => {
                events.push(AgentEvent::Delta { text: format!("⚠ {}", item["message"].as_str().unwrap_or_default()) });
            }
            ("turn.completed", _) => {
                let usage = &value["usage"];
                let count = |key: &str| usage[key].as_u64().unwrap_or(0);
                let cached = count("cached_input_tokens");
                events.push(AgentEvent::Usage {
                    input_tokens: count("input_tokens").saturating_sub(cached),
                    output_tokens: count("output_tokens"),
                    cache_read_tokens: cached,
                    cache_write_tokens: count("cache_write_input_tokens"),
                    new_call: true,
                });
                events.push(AgentEvent::Done { session_id: turn.thread.clone(), text: String::new(), cost_usd: None });
            }
            ("turn.failed", _) => events.push(AgentEvent::Error {
                message: value["error"]["message"].as_str().unwrap_or("Codex failed").to_string(),
            }),
            ("error", _) => events.push(AgentEvent::Error {
                message: value["message"].as_str().unwrap_or("Codex failed").to_string(),
            }),
            _ => {}
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(resume: bool) -> TurnRequest {
        TurnRequest {
            prompt: "hi".into(),
            session_id: Some("t1".into()),
            resume,
            model: Some("gpt-6-luna".into()),
            permission_mode: Some("acceptEdits".into()),
            effort: Some("high".into()),
            allowed_tools: Vec::new(),
            cwd: None,
        }
    }

    #[test]
    fn a_follow_up_turn_resumes_the_thread() {
        let args = Codex::default().args(&req(true));
        assert_eq!(&args[..3], ["exec", "resume", "t1"]);
        assert!(args.contains(&"-m".to_string()) && args.contains(&"gpt-6-luna".to_string()));
        assert!(args.contains(&"model_reasoning_effort=\"high\"".to_string()));
        assert!(args.contains(&"sandbox_mode=\"workspace-write\"".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("hi"));
        // A first turn starts a new thread: Codex names it.
        assert_eq!(Codex::default().args(&req(false))[1], "--json");
    }

    #[test]
    fn auto_approve_routes_mcp_approvals_to_automatic_review() {
        let mut request = req(false);
        request.permission_mode = Some("auto".into());
        let args = Codex::default().args(&request);
        assert!(args.contains(&"--approve-for-me".to_string()));
        assert!(!args.iter().any(|arg| arg == "approval_policy=\"never\""));
    }

    #[test]
    fn events_become_commands_text_usage_and_the_thread_id() {
        let codex = Codex::default();
        codex.args(&req(false));
        let lines = [
            r#"{"type":"thread.started","thread_id":"th-9"}"#,
            r#"{"type":"item.started","item":{"id":"item_0","type":"command_execution","command":"\"pwsh.exe\" -Command \"echo hi\"","status":"in_progress"}}"#,
            r#"{"type":"item.completed","item":{"id":"item_0","type":"command_execution","command":"x","aggregated_output":"hi","exit_code":0,"status":"completed"}}"#,
            r#"{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"done"}}"#,
            r#"{"type":"turn.completed","usage":{"input_tokens":100,"cached_input_tokens":60,"output_tokens":7}}"#,
        ];
        let events: Vec<AgentEvent> = lines.iter().flat_map(|line| codex.parse_line(line)).collect();
        assert!(matches!(&events[0], AgentEvent::ToolCall { input, .. } if input == "echo hi"));
        assert!(matches!(&events[1], AgentEvent::ToolResult { output, is_error: false, .. } if output == "hi"));
        assert!(matches!(&events[2], AgentEvent::Delta { text } if text == "done"));
        assert!(matches!(events[3], AgentEvent::Usage { input_tokens: 40, cache_read_tokens: 60, output_tokens: 7, .. }));
        assert!(matches!(&events[4], AgentEvent::Done { session_id: Some(id), .. } if id == "th-9"));
    }
}
