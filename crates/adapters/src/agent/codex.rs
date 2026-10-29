//! Codex, driven headless: `codex exec --json <prompt>`, one process per turn.
//!
//! Context across turns comes from the thread id the first turn reports (`thread.started`); later turns run
//! `codex exec resume <id>`. Options go through `-c key=value`, which both `exec` and `exec resume` accept.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use serde_json::Value;

use super::{line_diff, AgentEvent, FileChange, Provider, TurnRequest};

/// Added to Codex's instructions on every turn (the same facts Claude is given).
pub(super) const APP_CONTEXT: &str = "You are running inside AgentDeck, a desktop app with its own built-in browser (the Preview panel). To show a local dev server, inspect the workspace scripts (package.json, .claude/launch.json), identify the requested framework, then start it with the browser_serve tool (the command, the absolute project folder, the address it answers at): AgentDeck runs the server itself, so it keeps running after your turn, and opens the page in Preview. Never run a dev server in your own shell: it dies when your command or turn ends and Preview is left on a refused connection. If browser_serve reports a failed start, read the log it returns, fix the cause (config, missing install, port) and call it again; do not stop at the first error. If the page is already served, browser_open opens it. Both open the page in the current Preview tab. Pass new_tab: true to open it in a tab of its own instead, so the current tab stays as it is: when another chat is driving the current tab, when you compare or debug two pages side by side, or when you look something up. Then keep going in this same turn: screenshot, act on the page, screenshot again. Name the script you used in your reply. In the z-ui workspace, use `pnpm start:svelte` for the Svelte 5 app (default http://localhost:4301) and `pnpm start:angular` for the Angular app (default http://localhost:4200); never use the Angular 4200 server when the user asks for Svelte, Svelte 5, or z-ui-svelte. When the user asks you to inspect or interact with that page, you MUST use the AgentDeck MCP tools on the Preview page: call browser_screenshot first, then browser_click/browser_fill/browser_key/browser_scroll as needed (browser_fill for form fields; browser_type only when the page reacts to each keystroke), and call browser_screenshot or browser_dom again to verify the result. Do not use an external Chrome tab, do not merely navigate to or mention the URL, and do not claim an interaction happened without a matching MCP result. You can also use browser_eval, browser_console, and browser_network for inspection. Never tell the user you cannot test in Preview. A code index of the project is built in. To find your way, prefer the agentdeck tools code_search (a symbol by name), code_outline (the structure of a file), code_definition (a symbol with its source), code_references (where a name is used), code_imports and code_importers (what breaks if you change a file) over reading files or grepping the whole tree, then Read only what matters. Before you use a library API, check the version the project has (package.json, Cargo.toml) and look up the current documentation when you are not sure it exists in that version.";

/// What one run of the parser remembers between lines: the thread id for the final event, which commands
/// were already announced, and whether the last thing written was answer text.
#[derive(Default)]
struct Turn {
    thread: Option<String>,
    started: HashSet<String>,
    /// What each running command has printed so far: Codex reports the whole output every time.
    command_output: std::collections::HashMap<String, String>,
    last_was_text: bool,
    /// Folder the turn runs in: a changed file is read from here.
    cwd: Option<PathBuf>,
}

impl Turn {
    /// The part of a command's `output` not sent yet, as a [`AgentEvent::ToolOutputDelta`].
    fn output_delta(&mut self, id: &str, output: &str) -> Option<AgentEvent> {
        let previous = self.command_output.entry(id.to_owned()).or_default();
        let delta = output
            .strip_prefix(previous.as_str())
            .filter(|delta| !delta.is_empty())
            .map(|delta| AgentEvent::ToolOutputDelta {
                id: id.to_owned(),
                delta: delta.to_owned(),
            });
        *previous = output.to_owned();
        delta
    }
}

#[derive(Default)]
pub struct Codex {
    turn: Mutex<Turn>,
}

/// Codex prints `Reconnecting... 2/5 (request timed out)` as an `error` line while it retries the request itself.
/// That is progress, not the end of the turn: the turn ends with `turn.failed` or `turn.completed`.
fn is_retry_notice(message: &str) -> bool {
    let rest = message.trim_start().strip_prefix("Reconnecting");
    rest.is_some_and(|rest| rest.trim_start_matches('.').trim_start_matches('…').trim_start().starts_with(|c: char| c.is_ascii_digit()))
}

/// TOML string for a `-c key=value` override.
fn toml_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}

/// Where the native Codex binary lives. The npm and pnpm launchers are scripts around it; starting the binary
/// itself keeps arguments (newlines, quotes) intact on Windows, where batch files mangle them.
fn candidates_for(
    os: &str,
    home: Option<PathBuf>,
    local: Option<PathBuf>,
    appdata: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if os == "windows" {
        // The native binary is built per CPU: a Windows on ARM runs the arm64 one.
        let (triple, platform) = if std::env::consts::ARCH == "aarch64" {
            ("aarch64-pc-windows-msvc", "-win32-arm64")
        } else {
            ("x86_64-pc-windows-msvc", "-win32-x64")
        };
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
                            .is_some_and(|name| {
                                name.starts_with("@openai+codex@") && name.ends_with(platform)
                            })
                    })
                    .collect();
                versions.sort_by_key(|path| version_key(path));
                for version in versions.into_iter().rev() {
                    let vendor = version
                        .join("node_modules")
                        .join("@openai")
                        .join("codex")
                        .join("vendor")
                        .join(triple);
                    found.push(vendor.join("bin").join("codex.exe"));
                    found.push(vendor.join("codex").join("codex.exe"));
                }
            }
        }
        if let Some(appdata) = &appdata {
            let vendor = appdata
                .join("npm")
                .join("node_modules")
                .join("@openai")
                .join("codex")
                .join("vendor")
                .join(triple);
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
            let quote = marker.chars().last().unwrap_or('"');
            return inner.strip_suffix(quote).unwrap_or(inner).to_string();
        }
    }
    command.to_string()
}

/// Numeric order for `@openai+codex@<version>-…` folder names, so `0.10.0` sorts after `0.9.0`.
fn version_key(path: &Path) -> Vec<u64> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("@openai+codex@"))
        .and_then(|rest| rest.split('-').next())
        .map(|version| {
            version
                .split('.')
                .map(|part| part.parse().unwrap_or(0))
                .collect()
        })
        .unwrap_or_default()
}

/// Codex reports a changed file after writing it, so the text before the change comes from git's HEAD copy
/// (empty for a new file or outside a repository) and the text after from disk.
fn snapshot(cwd: Option<&Path>, path: &str) -> (String, String) {
    let file = Path::new(path);
    let full = match cwd {
        Some(cwd) if file.is_relative() => cwd.join(file),
        _ => file.to_path_buf(),
    };
    let after = std::fs::read_to_string(&full).unwrap_or_default();
    let before = cwd
        .and_then(|cwd| {
            let rel = full.strip_prefix(cwd).ok()?;
            Command::new("git")
                .arg("-C")
                .arg(cwd)
                .arg("show")
                .arg(format!(
                    "HEAD:./{}",
                    rel.to_string_lossy().replace('\\', "/")
                ))
                .output()
                .ok()
        })
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    (before, after)
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
        candidates_for(
            std::env::consts::OS,
            home,
            env_path("LOCALAPPDATA"),
            env_path("APPDATA"),
        )
    }

    fn args(&self, req: &TurnRequest) -> Vec<String> {
        if let Ok(mut turn) = self.turn.lock() {
            *turn = Turn {
                cwd: req.cwd.clone(),
                ..Turn::default()
            };
        }
        let mut args = vec!["exec".to_string()];
        let mode = super::permission::Mode::parse(req.permission_mode.as_deref());
        if mode == super::permission::Mode::Auto {
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
        if let Some(sandbox) = mode.codex_sandbox() {
            config("sandbox_mode", toml_string(sandbox));
            if mode != super::permission::Mode::Auto {
                config("approval_policy", toml_string("never"));
            }
        }
        config(
            "developer_instructions",
            toml_string(&format!("{APP_CONTEXT}{}", super::claude::ASK_CONTEXT)),
        );
        if let Ok(bridge) = std::env::var("AGENTDECK_BRIDGE") {
            config(
                "mcp_servers.agentdeck.url",
                toml_string(&super::mcp_url(
                    &bridge,
                    req.owner.as_deref(),
                    req.cwd.as_deref(),
                )),
            );
            // Codex gives an MCP tool 60 s by default; delegate_task waits for another agent for many minutes.
            config("mcp_servers.agentdeck.tool_timeout_sec", "7500".to_string());
        }
        if mode == super::permission::Mode::Yolo {
            args.push("--dangerously-bypass-approvals-and-sandbox".to_string());
        }
        // `-` makes `exec` read the prompt from stdin (see `prompt_on_stdin`); `--` keeps it from reading as a flag.
        args.push("--".to_string());
        args.push("-".to_string());
        args
    }

    fn prompt_on_stdin(&self) -> bool {
        true
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
                turn.command_output.insert(id.clone(), String::new());
                turn.last_was_text = false;
                events.push(AgentEvent::ToolCall {
                    id,
                    name: "Bash".into(),
                    input: bare_command(item["command"].as_str().unwrap_or_default()),
                    change: None,
                    background: false,
                });
            }
            ("item.updated", "command_execution") => {
                let output = item["aggregated_output"].as_str().unwrap_or_default();
                events.extend(turn.output_delta(&id, output));
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
                let failed = item["exit_code"].as_i64().is_some_and(|code| code != 0)
                    || item["status"] == "failed";
                // The result carries the whole output, so no last delta is sent: the streamed copy is dropped.
                let output = item["aggregated_output"].as_str().unwrap_or_default();
                turn.command_output.remove(&id);
                events.push(AgentEvent::ToolResult {
                    id,
                    output: output.to_string(),
                    is_error: failed,
                    image: None,
                    change: None,
                });
                turn.last_was_text = false;
            }
            ("item.completed", "agent_message") => {
                let text = item["text"].as_str().unwrap_or_default();
                if !text.is_empty() {
                    // Two messages in a row are two paragraphs, not one run-on line.
                    let joined = if turn.last_was_text {
                        format!("\n\n{text}")
                    } else {
                        text.to_string()
                    };
                    events.push(AgentEvent::Delta { text: joined });
                    turn.last_was_text = true;
                }
            }
            ("item.completed", "reasoning") => {
                let text = item["text"].as_str().unwrap_or_default();
                if !text.is_empty() {
                    events.push(AgentEvent::Thinking {
                        text: text.to_string(),
                    });
                    turn.last_was_text = false;
                }
            }
            ("item.completed", "file_change") => {
                let changes = item["changes"].as_array().cloned().unwrap_or_default();
                let cwd = turn.cwd.clone();
                for (index, change) in changes.iter().enumerate() {
                    let path = change["path"].as_str().unwrap_or_default().to_string();
                    let call = format!("{id}:{index}");
                    let name = match change["kind"].as_str() {
                        Some("add") => "Write",
                        Some("delete") => "Delete",
                        _ => "Edit",
                    };
                    let (before, after) = snapshot(cwd.as_deref(), &path);
                    let diff = line_diff(&before, &after);
                    events.push(AgentEvent::ToolCall {
                        id: call.clone(),
                        name: name.into(),
                        input: path.clone(),
                        change: Some(FileChange {
                            path,
                            added: diff.added,
                            removed: diff.removed,
                            before: diff.before,
                            after: diff.after,
                        }),
                        background: false,
                    });
                    events.push(AgentEvent::ToolResult {
                        id: call,
                        output: String::new(),
                        is_error: item["status"] == "failed",
                        image: None,
                        change: None,
                    });
                }
                turn.last_was_text = false;
            }
            ("item.started", "mcp_tool_call") | ("item.completed", "mcp_tool_call") => {
                let name = format!(
                    "{}.{}",
                    item["server"].as_str().unwrap_or("mcp"),
                    item["tool"].as_str().unwrap_or_default()
                );
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
                        .map(|parts| {
                            parts
                                .iter()
                                .filter_map(|part| part["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default();
                    events.push(AgentEvent::ToolResult {
                        id,
                        output: if output.is_empty() {
                            item["error"]["message"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string()
                        } else {
                            output
                        },
                        is_error: item["status"] == "failed",
                        image: super::result_image(&item["result"]["content"]),
                        change: None,
                    });
                }
                turn.last_was_text = false;
            }
            // The search shows as it runs (its start), so the live line can say what it is searching for.
            ("item.started", "web_search") | ("item.completed", "web_search") => {
                if turn.started.insert(id.clone()) {
                    events.push(AgentEvent::ToolCall {
                        id: id.clone(),
                        name: "WebSearch".into(),
                        // The query may be missing on the started item; `action.query` is read as a fallback.
                        input: item["query"]
                            .as_str()
                            .or_else(|| item["action"]["query"].as_str())
                            .unwrap_or_default()
                            .to_string(),
                        change: None,
                        background: false,
                    });
                }
                if value["type"] == "item.completed" {
                    events.push(AgentEvent::ToolResult {
                        id,
                        output: String::new(),
                        is_error: false,
                        image: None,
                        change: None,
                    });
                }
                turn.last_was_text = false;
            }
            ("item.completed", "error") => {
                events.push(AgentEvent::Delta {
                    text: format!("⚠ {}", item["message"].as_str().unwrap_or_default()),
                });
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
                events.push(AgentEvent::Done {
                    session_id: turn.thread.clone(),
                    text: String::new(),
                    cost_usd: None,
                });
            }
            ("turn.failed", _) => events.push(AgentEvent::Error {
                message: value["error"]["message"]
                    .as_str()
                    .unwrap_or("Codex failed")
                    .to_string(),
            }),
            ("error", _) => {
                let message = value["message"].as_str().unwrap_or("Codex failed");
                if is_retry_notice(message) {
                    // Codex is retrying the request itself ("Reconnecting... 2/5"): the turn is still running and
                    // usually finishes. Ending it here would leave the reply stuck while Codex goes on.
                    events.push(AgentEvent::Thinking {
                        text: format!("{message}\n"),
                    });
                } else {
                    events.push(AgentEvent::Error {
                        message: message.to_string(),
                    });
                }
            }
            _ => {}
        }
        events
    }

    fn chat_capabilities(&self, _req: &TurnRequest) -> super::ChatResponseCapabilities {
        super::ChatResponseCapabilities::codex()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
            owner: None,
            api_key: None,
        }
    }

    #[test]
    fn a_follow_up_turn_resumes_the_thread() {
        let args = Codex::default().args(&req(true));
        assert_eq!(&args[..3], ["exec", "resume", "t1"]);
        assert!(args.contains(&"-m".to_string()) && args.contains(&"gpt-6-luna".to_string()));
        assert!(args.contains(&"model_reasoning_effort=\"high\"".to_string()));
        assert!(args.contains(&"sandbox_mode=\"workspace-write\"".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("-"));
        assert!(!args.contains(&"hi".to_string()));
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
        let events: Vec<AgentEvent> = lines
            .iter()
            .flat_map(|line| codex.parse_line(line))
            .collect();
        assert!(matches!(&events[0], AgentEvent::ToolCall { input, .. } if input == "echo hi"));
        assert!(
            matches!(&events[1], AgentEvent::ToolResult { output, is_error: false, .. } if output == "hi")
        );
        assert!(matches!(&events[2], AgentEvent::Delta { text } if text == "done"));
        assert!(matches!(
            events[3],
            AgentEvent::Usage {
                input_tokens: 40,
                cache_read_tokens: 60,
                output_tokens: 7,
                ..
            }
        ));
        assert!(
            matches!(&events[4], AgentEvent::Done { session_id: Some(id), .. } if id == "th-9")
        );
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("agentdeck-codex-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_prompt_follows_a_separator_so_a_dash_cannot_read_as_a_flag() {
        let mut request = req(false);
        request.prompt = "-x fix it".into();
        let args = Codex::default().args(&request);
        // The prompt itself goes to stdin, so the command line stays short.
        assert_eq!(&args[args.len() - 2..], ["--", "-"]);
        assert!(Codex::default().prompt_on_stdin());
        assert!(!args.iter().any(|arg| arg.contains("-x fix it")));
    }

    #[test]
    fn a_shell_wrapper_comes_off_but_inner_quotes_stay() {
        assert_eq!(bare_command("/bin/zsh -lc 'echo \"x\"'"), "echo \"x\"");
        assert_eq!(bare_command("bash -lc 'ls -la'"), "ls -la");
        assert_eq!(bare_command("cargo test"), "cargo test");
    }

    #[test]
    fn a_new_file_becomes_a_write_and_a_removed_one_a_delete() {
        let dir = temp_dir("files");
        std::fs::write(dir.join("new.txt"), "one\ntwo\n").unwrap();
        let codex = Codex::default();
        let mut request = req(false);
        request.cwd = Some(dir.clone());
        codex.args(&request);
        let line = json!({
            "type": "item.completed",
            "item": {
                "id": "f1",
                "type": "file_change",
                "status": "completed",
                "changes": [{"path": "new.txt", "kind": "add"}, {"path": "gone.txt", "kind": "delete"}]
            }
        })
        .to_string();
        let events = codex.parse_line(&line);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            matches!(&events[0], AgentEvent::ToolCall { name, change: Some(change), .. }
            if name == "Write" && change.path == "new.txt" && change.added == 2
                && change.before.is_empty() && change.after == "one\ntwo")
        );
        assert!(
            matches!(&events[2], AgentEvent::ToolCall { name, change: Some(change), .. }
            if name == "Delete" && change.path == "gone.txt" && change.after.is_empty())
        );
    }

    #[test]
    fn a_failed_tool_call_is_announced_once_and_reports_its_error() {
        let codex = Codex::default();
        codex.args(&req(false));
        let started = json!({
            "type": "item.started",
            "item": {"id": "m1", "type": "mcp_tool_call", "server": "agentdeck", "tool": "browser_click", "arguments": {"ref": "ref_3"}, "status": "in_progress"}
        })
        .to_string();
        let completed = json!({
            "type": "item.completed",
            "item": {"id": "m1", "type": "mcp_tool_call", "server": "agentdeck", "tool": "browser_click", "arguments": {"ref": "ref_3"}, "status": "failed", "error": {"message": "no page"}}
        })
        .to_string();
        let events: Vec<AgentEvent> = [started, completed]
            .iter()
            .flat_map(|line| codex.parse_line(line))
            .collect();
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[0], AgentEvent::ToolCall { name, .. } if name == "agentdeck.browser_click")
        );
        assert!(
            matches!(&events[1], AgentEvent::ToolResult { output, is_error: true, .. } if output == "no page")
        );
    }

    #[test]
    fn errors_reach_the_chat_and_a_failed_turn_has_no_done() {
        let codex = Codex::default();
        let note = codex.parse_line(&json!({"type": "item.completed", "item": {"id": "e1", "type": "error", "message": "rate limited"}}).to_string());
        assert!(matches!(&note[..], [AgentEvent::Delta { text }] if text == "⚠ rate limited"));
        let failed = codex
            .parse_line(&json!({"type": "turn.failed", "error": {"message": "quota"}}).to_string());
        assert!(matches!(&failed[..], [AgentEvent::Error { message }] if message == "quota"));
        let top = codex.parse_line(&json!({"type": "error", "message": "boom"}).to_string());
        assert!(matches!(&top[..], [AgentEvent::Error { message }] if message == "boom"));
    }

    #[test]
    fn a_reconnect_notice_does_not_end_the_turn() {
        let codex = Codex::default();
        let notice = codex.parse_line(&json!({"type": "error", "message": "Reconnecting... 2/5 (request timed out)"}).to_string());
        assert!(matches!(&notice[..], [AgentEvent::Thinking { text }] if text.starts_with("Reconnecting... 2/5")));
        // A real failure that mentions reconnecting is still a failure.
        let failed = codex.parse_line(&json!({"type": "error", "message": "Reconnecting failed: no network"}).to_string());
        assert!(matches!(&failed[..], [AgentEvent::Error { .. }]));
    }

    #[test]
    fn a_turn_without_usage_or_thread_still_ends_cleanly() {
        let events = Codex::default().parse_line(r#"{"type":"turn.completed"}"#);
        assert!(matches!(
            events[0],
            AgentEvent::Usage {
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                ..
            }
        ));
        assert!(matches!(
            &events[1],
            AgentEvent::Done {
                session_id: None,
                ..
            }
        ));
    }

    #[test]
    fn pnpm_installs_list_the_newest_codex_version_first() {
        let local = temp_dir("pnpm");
        let platform = if std::env::consts::ARCH == "aarch64" {
            "win32-arm64"
        } else {
            "win32-x64"
        };
        for version in ["0.9.0", "0.10.0"] {
            let folder = format!("@openai+codex@{version}-{platform}");
            std::fs::create_dir_all(
                local
                    .join("pnpm")
                    .join("global")
                    .join("5")
                    .join(".pnpm")
                    .join(folder),
            )
            .unwrap();
        }
        let found = candidates_for("windows", None, Some(local.clone()), None);
        std::fs::remove_dir_all(&local).unwrap();
        let newest = found
            .iter()
            .position(|path| path.to_string_lossy().contains("0.10.0"));
        let older = found
            .iter()
            .position(|path| path.to_string_lossy().contains("0.9.0"));
        assert!(
            newest.is_some() && older.is_some() && newest < older,
            "{found:?}"
        );
    }

    #[test]
    fn unix_looks_in_the_home_bin_before_the_system_folders() {
        let found = candidates_for("linux", Some(PathBuf::from("/home/me")), None, None);
        assert_eq!(found[0], PathBuf::from("/home/me/.local/bin/codex"));
        assert!(found.contains(&PathBuf::from("/usr/bin/codex")));
    }
}
