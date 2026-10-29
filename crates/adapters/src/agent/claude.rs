//! Claude Code, driven headless: `claude -p <prompt> --output-format stream-json`.
//!
//! One process per turn. Context across turns comes from the conversation id: the first turn passes
//! `--session-id <uuid>`, later turns pass `--resume <uuid>`.

use std::path::PathBuf;

use serde_json::Value;

use super::{
    allow_entry, file_change, summarize_tool_input, AgentEvent, PermissionRequest, Provider, TaskStatus,
    TurnRequest,
};

pub struct Claude;

/// Added to the CLI's system prompt on every turn.
/// How an agent asks the user multiple-choice questions in AgentDeck (rendered as a question card).
pub const ASK_CONTEXT: &str = r#" To ask the user before you plan (only when a wrong guess would waste work), end your reply with one fenced block ```agentdeck-ask holding {"questions":[{"id","header","question","multi":false,"options":[{"label","description"}]}]} (1-3 questions, 2-4 options, recommended first, no "Other") and stop: the app shows it as a question card. Your own question tool does not reach the user here."#;

const APP_CONTEXT: &str = "You are running inside AgentDeck, a desktop app that has its own built-in browser (the Preview panel). When the user asks you to open, preview, run or show a web page or a local dev server, do not open it with an external browser or any browser-automation tool. Find the start script that matches the framework the user asked for (package.json scripts, .claude/launch.json), then start it with the browser_serve tool (the command, the absolute project folder, the address it answers at): AgentDeck runs the server itself, so it keeps running after your turn, and opens the page in Preview. Never run a dev server in your own shell: it dies when your command or turn ends and Preview is left on a refused connection. If browser_serve reports a failed start, read the log it returns, fix the cause (config, missing install, port) and call it again; do not stop at the first error. If the page is already served, browser_open opens it. Both open the page in the current Preview tab. Pass new_tab: true to open it in a tab of its own instead, so the current tab stays as it is: when another chat is driving the current tab, when you compare or debug two pages side by side, or when you look something up. Then keep going in this same turn: screenshot, act on the page, screenshot again. Name the script you used in your reply. A code index of the project is built in. To find your way, prefer the agentdeck tools code_search (a symbol by name), code_outline (the structure of a file), code_definition (a symbol with its source), code_references (where a name is used), code_imports and code_importers (what breaks if you change a file) over reading files or grepping the whole tree, then Read only what matters. Before you use a library API, check the version the project has (package.json, Cargo.toml) and look up the current documentation when you are not sure it exists in that version.";

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

    fn prompt_on_stdin(&self) -> bool {
        true
    }

    fn environment(&self, req: &TurnRequest) -> Vec<(String, String)> {
        // No auto-update check, telemetry or feature-flag fetch on start-up: measured on a cold start, the first answer
        // comes 0.4 to 1.3 s sooner and the process ends faster. (It also turns Remote Control off: not used here.)
        let mut env = vec![("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(), "1".to_string())];
        // An API key wins over the signed-in plan in Claude Code's sign-in order: the turn spends Console credit.
        env.extend(
            req.api_key
                .iter()
                .filter(|key| !key.is_empty())
                .map(|key| ("ANTHROPIC_API_KEY".to_string(), key.clone())),
        );
        env
    }

    fn args(&self, req: &TurnRequest) -> Vec<String> {
        // `-p` with no prompt argument reads the prompt from stdin (see `prompt_on_stdin`).
        let mut args = vec![
            "-p".to_string(),
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
        let mut context = APP_CONTEXT.to_string();
        context.push_str(ASK_CONTEXT);
        let bridge = std::env::var("AGENTDECK_BRIDGE").ok();
        // In the z-ui workspace the two apps have their own start scripts (the bare `pnpm start` asks interactively).
        context.push_str(" In the z-ui workspace, start the app the user asked for with its own script: `pnpm start:svelte` for the Svelte 5 app (default http://localhost:4301) and `pnpm start:angular` for the Angular app (default http://localhost:4200). Never open the Angular server when the user asks for Svelte, Svelte 5 or z-ui-svelte.");
        if let Some(bridge) = &bridge {
            context.push_str(" You CAN see and operate the page open in the Preview panel yourself, with the agentdeck tools: browser_screenshot to look, browser_click / browser_fill (form fields; browser_type only when the page reacts to each keystroke) / browser_key / browser_scroll / browser_hover to act as a user would (coordinates are CSS pixels of the screenshot; browser_eval with getBoundingClientRect() finds an element's point), browser_eval and browser_dom to read the page, browser_console and browser_network for errors. Never tell the user you cannot click or test in Preview, and do not ask them to do it for you. Test step by step: look, act, look again, check the console and network.");
            context.push_str(&format!(
                " The same actions are plain HTTP too if a tool is missing: curl {bridge}/help."
            ));
        }
        args.push(context);
        // The AgentDeck browser as real tools (an MCP server served by the app), allowed without asking.
        if let Some(bridge) = &bridge {
            args.push("--mcp-config".to_string());
            args.push(
                serde_json::json!({ "mcpServers": { "agentdeck": { "type": "http", "url": super::mcp_url(bridge, req.owner.as_deref(), req.cwd.as_deref()) } } })
                    .to_string(),
            );
            args.push("--allowedTools".to_string());
            args.push("mcp__agentdeck".to_string());
        }
        if let Some(effort) = &req.effort {
            args.push("--effort".to_string());
            args.push(effort.clone());
        }
        if let Some(mode) = super::permission::Mode::parse(req.permission_mode.as_deref()).claude() {
            args.push("--permission-mode".to_string());
            args.push(mode.to_string());
        }
        for tool in &req.allowed_tools {
            args.push("--allowedTools".to_string());
            args.push(tool.clone());
        }
        // Claude runs its own commands, so the guard cannot look at them: its CLI refuses the heaviest ones by their start.
        for tool in super::guard::claude_denied_tools() {
            args.push("--disallowedTools".to_string());
            args.push(tool);
        }
        args
    }

    /// The same settings, with each message read as a JSON line: `-p` keeps the conversation going for every line.
    fn warm_args(&self, req: &TurnRequest) -> Option<Vec<String>> {
        let mut args = self.args(req);
        args.push("--input-format".to_string());
        args.push("stream-json".to_string());
        // The CLI reads each message of the user back when it takes it: how a turn knows an injected one was read.
        args.push("--replay-user-messages".to_string());
        Some(args)
    }

    fn warm_message(&self, prompt: &str) -> String {
        serde_json::json!({ "type": "user", "message": { "role": "user", "content": prompt } }).to_string()
    }

    fn interrupt_mode(&self) -> super::interrupt::InterruptMode {
        super::interrupt::InterruptMode::StreamJson
    }

    fn live_model(&self) -> bool {
        true
    }

    fn model_switch_line(&self, request_id: &str, model: &str) -> String {
        // The CLI's own `set_model` control request: it keeps the conversation and changes the model of the next turn.
        serde_json::json!({ "type": "control_request", "request_id": request_id, "request": { "subtype": "set_model", "model": model } }).to_string()
    }

    fn chat_capabilities(&self, _req: &TurnRequest) -> super::ChatResponseCapabilities {
        super::ChatResponseCapabilities::claude()
    }

    fn tail_provider(&self) -> Option<std::sync::Arc<dyn Provider>> {
        Some(std::sync::Arc::new(Claude))
    }

    fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let events = self.parse_value(&value, line);
        // A line of a sub-agent carries the id of the tool call that started it: its text and tools belong under that
        // card, not in the main answer.
        match value["parent_tool_use_id"].as_str().filter(|id| !id.is_empty()) {
            Some(parent) => events
                .into_iter()
                .map(|event| match event {
                    AgentEvent::Usage { .. } | AgentEvent::Raw { .. } | AgentEvent::Task { .. } => event,
                    inner => AgentEvent::Scoped { parent: parent.to_owned(), event: Box::new(inner) },
                })
                .collect(),
            None => events,
        }
    }
}

impl Claude {
    fn parse_value(&self, value: &Value, line: &str) -> Vec<AgentEvent> {
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
                    // A model call starts: its input (and cache use) is known now.
                    (Some("message_start"), _) => {
                        let usage = &value["event"]["message"]["usage"];
                        vec![AgentEvent::Usage {
                            input_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
                            output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
                            cache_read_tokens: usage["cache_read_input_tokens"]
                                .as_u64()
                                .unwrap_or(0),
                            cache_write_tokens: usage["cache_creation_input_tokens"]
                                .as_u64()
                                .unwrap_or(0),
                            new_call: true,
                        }]
                    }
                    // The call's output so far.
                    (Some("message_delta"), _) => {
                        match value["event"]["usage"]["output_tokens"].as_u64() {
                            Some(output) => vec![AgentEvent::Usage {
                                input_tokens: 0,
                                output_tokens: output,
                                cache_read_tokens: 0,
                                cache_write_tokens: 0,
                                new_call: false,
                            }],
                            None => Vec::new(),
                        }
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
                        background: block["input"]["run_in_background"].as_bool() == Some(true)
                            || matches!(name, "Agent" | "Task"),
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
                    image: super::result_image(&block["content"]),
                    change: None,
                })
                .collect(),
            // A sub-agent or a background job starts, reports on its way, and ends.
            Some("system")
                if matches!(value["subtype"].as_str(), Some("task_started" | "task_progress" | "task_notification")) =>
            {
                let text = |key: &str| value[key].as_str().filter(|text| !text.is_empty()).map(str::to_owned);
                let (status, summary) = match value["subtype"].as_str() {
                    Some("task_started") => (TaskStatus::Started, None),
                    Some("task_progress") => (TaskStatus::Progress, text("description")),
                    _ => (
                        if matches!(value["status"].as_str(), Some("completed")) { TaskStatus::Done } else { TaskStatus::Failed },
                        text("summary"),
                    ),
                };
                let label = match (text("subagent_type"), text("description")) {
                    (Some(kind), Some(what)) if status == TaskStatus::Started => Some(format!("{kind}: {what}")),
                    (Some(kind), _) => Some(kind),
                    (None, what) => what.filter(|_| status == TaskStatus::Started),
                };
                vec![AgentEvent::Task {
                    id: text("task_id").unwrap_or_default(),
                    tool_use_id: text("tool_use_id"),
                    status,
                    label,
                    summary,
                    tools: value["usage"]["tool_uses"].as_u64().and_then(|count| u32::try_from(count).ok()),
                }]
            }
            // The CLI moved a turn to another model (a refusal, an overload, a model that is gone). Passed on as it is:
            // the chat says so and, when the whole session moved (`scope` other than "local"), follows the new model.
            Some("system")
                if matches!(value["subtype"].as_str(), Some("model_refusal_fallback" | "model_fallback")) =>
            {
                vec![AgentEvent::Raw {
                    source: "claude".to_string(),
                    label: value["subtype"].as_str().unwrap_or_default().to_string(),
                    line: line.to_string(),
                }]
            }
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
            owner: None,
            api_key: None,
        }
    }

    #[test]
    fn an_api_key_reaches_the_cli_as_its_environment() {
        let quiet = ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".to_string(), "1".to_string());
        assert_eq!(Claude.environment(&req(false)), vec![quiet.clone()]);
        let mut with_key = req(false);
        with_key.api_key = Some("sk-ant-test".into());
        assert_eq!(
            Claude.environment(&with_key),
            vec![quiet, ("ANTHROPIC_API_KEY".to_string(), "sk-ant-test".to_string())]
        );
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
        let start = r#"{"type":"stream_event","event":{"type":"message_start","message":{"usage":{"input_tokens":12,"cache_read_input_tokens":900,"cache_creation_input_tokens":40,"output_tokens":1}}}}"#;
        assert_eq!(
            Claude.parse_line(start),
            vec![AgentEvent::Usage {
                input_tokens: 12,
                output_tokens: 1,
                cache_read_tokens: 900,
                cache_write_tokens: 40,
                new_call: true
            }]
        );
        let delta = r#"{"type":"stream_event","event":{"type":"message_delta","usage":{"output_tokens":321}}}"#;
        assert_eq!(
            Claude.parse_line(delta),
            vec![AgentEvent::Usage {
                input_tokens: 0,
                output_tokens: 321,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                new_call: false
            }]
        );
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
    fn a_model_fallback_is_passed_on_and_other_system_lines_are_not() {
        let line = r#"{"type":"system","subtype":"model_refusal_fallback","direction":"retry","scope":"local","original_model":"opus","fallback_model":"sonnet"}"#;
        assert!(matches!(Claude.parse_line(line).as_slice(), [AgentEvent::Raw { label, .. }] if label == "model_refusal_fallback"));
        assert!(Claude.parse_line(r#"{"type":"system","subtype":"init"}"#).is_empty());
    }

    #[test]
    fn a_model_switch_is_the_cli_set_model_control_request() {
        let line = Claude.model_switch_line("agentdeck-model-1", "sonnet");
        assert!(!line.contains('\n'));
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["type"], "control_request");
        assert_eq!(value["request_id"], "agentdeck-model-1");
        assert_eq!(value["request"]["subtype"], "set_model");
        assert_eq!(value["request"]["model"], "sonnet");
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
                change: None,
                background: false
            }]
        );
    }

    #[test]
    fn background_shells_and_sub_agents_are_marked() {
        let shell = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"b","name":"Bash","input":{"command":"pnpm dev","run_in_background":true}}]}}"#;
        let agent = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"a","name":"Agent","input":{"prompt":"look"}}]}}"#;
        for line in [shell, agent] {
            assert!(matches!(
                Claude.parse_line(line)[0],
                AgentEvent::ToolCall {
                    background: true,
                    ..
                }
            ));
        }
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
                is_error: true,
                image: None,
                change: None,
            }]
        );
        // A screenshot comes back as an image block: it rides along as a data URL.
        let shot = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAA"}}]}]}}"#;
        assert!(matches!(
            &Claude.parse_line(shot)[0],
            AgentEvent::ToolResult { image: Some(image), .. } if image == "data:image/png;base64,AAA"
        ));
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

    #[test]
    fn a_sub_agents_lines_are_scoped_to_the_tool_call_that_started_it() {
        let call = r#"{"type":"assistant","parent_tool_use_id":"toolu_agent","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"ls"}}]}}"#;
        match Claude.parse_line(call).as_slice() {
            [AgentEvent::Scoped { parent, event }] => {
                assert_eq!(parent, "toolu_agent");
                assert!(matches!(**event, AgentEvent::ToolCall { .. }));
            }
            other => panic!("expected a scoped tool call, got {other:?}"),
        }
        let top = r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#;
        assert!(matches!(Claude.parse_line(top).as_slice(), [AgentEvent::ToolCall { .. }]));
    }

    #[test]
    fn task_lines_report_where_a_sub_agent_stands() {
        let started = r#"{"type":"system","subtype":"task_started","task_id":"a1","tool_use_id":"toolu_agent","description":"Scan docs","subagent_type":"Explore"}"#;
        assert!(matches!(
            Claude.parse_line(started).as_slice(),
            [AgentEvent::Task { status: TaskStatus::Started, label: Some(label), tool_use_id: Some(_), .. }] if label == "Explore: Scan docs"
        ));
        let progress = r#"{"type":"system","subtype":"task_progress","task_id":"a1","tool_use_id":"toolu_agent","description":"Reading","usage":{"tool_uses":3}}"#;
        assert!(matches!(Claude.parse_line(progress).as_slice(), [AgentEvent::Task { status: TaskStatus::Progress, tools: Some(3), .. }]));
        let done = r#"{"type":"system","subtype":"task_notification","task_id":"a1","tool_use_id":"toolu_agent","status":"completed","summary":"Found 4 files"}"#;
        assert!(matches!(
            Claude.parse_line(done).as_slice(),
            [AgentEvent::Task { status: TaskStatus::Done, summary: Some(summary), .. }] if summary == "Found 4 files"
        ));
    }

    #[test]
    fn open_background_work_is_read_from_the_task_list() {
        use crate::agent::tail::background_open;
        assert_eq!(background_open(r#"{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"a"}]}"#), Some(true));
        assert_eq!(background_open(r#"{"type":"system","subtype":"background_tasks_changed","tasks":[]}"#), Some(false));
        assert_eq!(background_open(r#"{"type":"result"}"#), None);
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
