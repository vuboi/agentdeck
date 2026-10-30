//! Antigravity, driven headless: `agy -p <prompt> --output-format stream-json`, one process per turn.
//!
//! Its stream is not Claude's. Every line is `{"event": ...}`: `init` (the conversation id), `step_update` (one step
//! of the run: the user's input, a piece of the answer, a tool call, with `ACTIVE` then `DONE` or `ERROR`) and
//! `result` (the end of the turn). A new conversation takes no id from us: `agy` names it, and a later turn
//! continues it with `--conversation <id>`.
//!
//! Permissions: in a headless run `agy` cannot ask, so a command it is not allowed to run is refused and listed in
//! the result. Approving it writes the exact rule `command(<command>)` into `permissions.allow` of agy's own
//! `settings.json` before the next turn (`prepare`), which allows that command and nothing else (checked: a longer
//! or chained command is still refused). The bypass flag is only used when the user chose the bypass mode.
//!
//! Fixtures of real runs (agy 1.4.5, paths and tool lists redacted) are in `tests/fixtures/agy`.

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

use super::{AgentEvent, PermissionRequest, Provider, TaskStatus, TurnRequest};

/// What an approval card hands back for a refused command: `agy:command(<command>)`.
const ALLOW_PREFIX: &str = "agy:command(";

/// The conversation id for the final event, and which tool steps were already announced.
#[derive(Default)]
struct Turn {
    conversation: Option<String>,
    started: std::collections::HashSet<u64>,
    root: Option<PathBuf>,
    file_snapshots: std::collections::HashMap<u64, super::FileSnapshot>,
    /// Commands `agy` refused to run, in order.
    refused: Vec<String>,
}

#[derive(Default)]
pub struct Antigravity {
    turn: Mutex<Turn>,
}

/// Where `agy` installs itself: `%LOCALAPPDATA%\agy\bin` on Windows, `~/.local/bin` elsewhere.
fn candidates_for(os: &str, home: Option<PathBuf>, local: Option<PathBuf>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if os == "windows" {
        if let Some(local) = local {
            found.push(local.join("agy").join("bin").join("agy.exe"));
        }
    } else if let Some(home) = home {
        found.push(home.join(".local").join("bin").join("agy"));
    }
    found
}

/// The names the chat knows tools by (it shows Read, Edit, Bash…), for the ones `agy` calls its own way.
fn tool_name(name: &str) -> &str {
    match name {
        "read_file" | "view_file" => "Read",
        "write_file" => "Write",
        "edit_file" | "replace_file_content" => "Edit",
        "run_command" => "Bash",
        "grep_search" => "Grep",
        "find_by_name" => "Glob",
        other => other,
    }
}

/// One line of what a tool was asked to do: its command or file, else its parameters.
fn tool_input(parameters: &Value) -> String {
    let picked = [
        "CommandLine",
        "Path",
        "AbsolutePath",
        "TargetFile",
        "Pattern",
        "Query",
        "Url",
    ]
    .iter()
    .find_map(|key| parameters.get(*key)?.as_str().map(str::to_owned));
    picked.unwrap_or_else(|| super::summarize_tool_input("", parameters))
}

/// Whether `agy` can be given a rule for this command without the rule meaning more than the command: a newline or
/// a parenthesis would end the `command(...)` early, and an empty one allows nothing in particular.
fn ruleable(command: &str) -> bool {
    !command.trim().is_empty() && !command.contains(['\n', '\r', '(', ')'])
}

/// The `permissions.allow` rule an approval stands for, if the approval is one of ours and safe to write.
fn allow_rule(allowed_tool: &str) -> Option<String> {
    let command = allowed_tool.strip_prefix(ALLOW_PREFIX)?.strip_suffix(')')?;
    ruleable(command).then(|| format!("command({command})"))
}

/// Adds the rules that are not there yet to `permissions.allow` in agy's settings file, keeping everything else in
/// it. The first time, the file is copied to `settings.json.agentdeck.bak`. A file that is not valid JSON is left alone.
pub fn add_allow_rules(path: &std::path::Path, rules: &[String]) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::from("{}"),
        Err(error) => return Err(format!("could not read agy's settings: {error}")),
    };
    let mut settings: Value = serde_json::from_str(&text).map_err(|_| {
        "agy's settings.json is not valid JSON, so the permission was not added".to_string()
    })?;
    let allow = settings
        .as_object_mut()
        .ok_or("agy's settings.json is not an object")?
        .entry("permissions")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("`permissions` in agy's settings.json is not an object")?
        .entry("allow")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or("`permissions.allow` in agy's settings.json is not a list")?;
    let before = allow.len();
    for rule in rules {
        if !allow.iter().any(|existing| existing.as_str() == Some(rule)) {
            allow.push(Value::String(rule.clone()));
        }
    }
    if allow.len() == before {
        return Ok(());
    }
    let failed = |error: std::io::Error| format!("could not write agy's settings: {error}");
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(failed)?;
    }
    let backup = path.with_extension("json.agentdeck.bak");
    if path.exists() && !backup.exists() {
        std::fs::copy(path, &backup).map_err(failed)?;
    }
    let scratch = path.with_extension("json.agentdeck.tmp");
    std::fs::write(
        &scratch,
        serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?,
    )
    .map_err(failed)?;
    std::fs::rename(&scratch, path).map_err(failed)
}

/// `--mode` for a permission mode of the app (`None`: the default mode of `agy`).
fn mode_flag(permission_mode: &str) -> Option<&'static str> {
    super::permission::Mode::parse(Some(permission_mode)).agy()
}

/// A model id that already carries its reasoning level (`gemini-3.8-flash-low`) conflicts with `--effort`.
fn has_effort_suffix(model: &str) -> bool {
    ["-low", "-medium", "-high", "-xhigh", "-max"]
        .iter()
        .any(|suffix| model.ends_with(suffix))
}

impl Provider for Antigravity {
    fn id(&self) -> &'static str {
        "antigravity"
    }

    fn binary(&self) -> &'static str {
        "agy"
    }

    fn candidates(&self) -> Vec<PathBuf> {
        let env_path = |key: &str| std::env::var_os(key).map(PathBuf::from);
        let home = env_path("HOME").or_else(|| env_path("USERPROFILE"));
        candidates_for(std::env::consts::OS, home, env_path("LOCALAPPDATA"))
    }

    fn args(&self, req: &TurnRequest) -> Vec<String> {
        if let Ok(mut turn) = self.turn.lock() {
            *turn = Turn::default();
        }
        let mut args = vec![
            "-p".to_string(),
            super::turn_prompt(self, req),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];
        // `agy` names a new conversation itself (an unknown id only starts a new one with a warning), so an id
        // is passed only to continue one it reported before.
        if let (Some(id), true) = (&req.session_id, req.resume) {
            args.push("--conversation".to_string());
            args.push(id.clone());
        }
        if let Some(model) = &req.model {
            args.push("--model".to_string());
            args.push(model.clone());
            if let Some(effort) = req.effort.as_ref().filter(|_| !has_effort_suffix(model)) {
                args.push("--effort".to_string());
                args.push(effort.clone());
            }
        }
        let mode = req.permission_mode.as_deref().unwrap_or_default();
        if super::permission::Mode::parse(Some(mode)) == super::permission::Mode::Yolo {
            args.push("--dangerously-skip-permissions".to_string());
        } else if let Some(mode) = mode_flag(mode) {
            args.push("--mode".to_string());
            args.push(mode.to_string());
        }
        // A tool waiting for an answer nobody can give must not hold the turn for ever.
        args.push("--print-timeout".to_string());
        args.push("30m".to_string());
        args
    }

    /// Antigravity has no system prompt flag here, so the app and ask instructions open each new conversation.
    fn opening_context(&self, req: &TurnRequest) -> Option<String> {
        let continues = req.resume && req.session_id.is_some();
        (!continues).then(|| format!("{}{}", super::codex::APP_CONTEXT, super::claude::ASK_CONTEXT))
    }

    /// The same settings with each message read as a JSON line (`--input-format stream-json`, agy 1.1.15+). The prompt
    /// flag stays, empty and attached (`-p=`), so print mode is on without taking the next flag as its prompt.
    fn warm_args(&self, req: &TurnRequest) -> Option<Vec<String>> {
        let mut args = self.args(req);
        args.splice(0..2, ["--input-format".to_string(), "stream-json".to_string(), "-p=".to_string()]);
        Some(args)
    }

    fn warm_message(&self, prompt: &str) -> String {
        serde_json::json!({ "event": "user", "message": { "role": "user", "content": [{ "type": "text", "text": prompt }] } }).to_string()
    }

    fn prepare(&self, req: &TurnRequest) -> Result<(), String> {
        if let Ok(mut turn) = self.turn.lock() {
            turn.root = req.cwd.clone();
            turn.file_snapshots.clear();
        }
        let rules: Vec<String> = req
            .allowed_tools
            .iter()
            .filter_map(|tool| allow_rule(tool))
            .collect();
        if rules.is_empty() {
            return Ok(());
        }
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or("no home folder to find agy's settings in")?;
        add_allow_rules(
            &home
                .join(".gemini")
                .join("antigravity-cli")
                .join("settings.json"),
            &rules,
        )
    }

    fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let Ok(mut turn) = self.turn.lock() else {
            return Vec::new();
        };
        match value["event"].as_str().unwrap_or_default() {
            "init" => {
                turn.conversation = value["conversation_id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned);
                Vec::new()
            }
            "step_update" => step(&value["step_update"], &mut turn),
            "result" => finish(&value["result"], &turn),
            _ => Vec::new(),
        }
    }

    fn chat_capabilities(&self, _req: &TurnRequest) -> super::ChatResponseCapabilities {
        super::ChatResponseCapabilities::antigravity()
    }
}

/// The output of `agy models` (it asks the server, so it can take a few seconds). Empty when `agy` is missing,
/// not signed in, or does not answer in time.
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

/// A `%XX`-escaped path (as in a `file://` address) with the escapes resolved.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%' && index + 2 < bytes.len())
            .then(|| {
                text.get(index + 1..index + 3)
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            })
            .flatten();
        match escaped {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The picture a tool returned. `agy` saves it in its own folder and says so in the tool's output, as
/// `[Resource offloaded to file:///C:/Users/me/.gemini/antigravity-cli/brain/<id>/…/media_0.png]`. It becomes a
/// `data:` URL for the thumbnail, but only when the file is inside `<home>/.gemini` (the output is text from a tool
/// and could name any path) and is a picture of at most 12 MB.
fn offloaded_image_in(output: &str, home: &std::path::Path) -> Option<String> {
    use base64::Engine;
    let from = output.find("file://")? + "file://".len();
    let rest = &output[from..];
    let address = rest.split([']', '\n', ' ']).next()?;
    let mut path = percent_decode(address);
    // `file:///C:/x` leaves `/C:/x`: drop the slash before a drive letter.
    if path.len() > 2 && path.starts_with('/') && path.as_bytes()[2] == b':' {
        path.remove(0);
    }
    let mime = match path.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return None,
    };
    let path = std::path::Path::new(&path).canonicalize().ok()?;
    if !path.starts_with(home.join(".gemini").canonicalize().ok()?) {
        return None;
    }
    if std::fs::metadata(&path).ok()?.len() > 12 * 1024 * 1024 {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn offloaded_image(output: &str) -> Option<String> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    offloaded_image_in(output, std::path::Path::new(&home))
}

/// `call_mcp_tool` carries the server and the tool in its parameters: shown like Claude shows them, as
/// `mcp__<server>__<tool>`, with the tool's own arguments as the input.
fn mcp_call(parameters: &Value) -> Option<(String, String)> {
    let server = parameters["ServerName"].as_str()?;
    let tool = parameters["ToolName"].as_str()?;
    Some((
        format!("mcp__{server}__{tool}"),
        super::summarize_tool_input("", &parameters["Arguments"]),
    ))
}

fn step(update: &Value, turn: &mut Turn) -> Vec<AgentEvent> {
    let state = update["state"].as_str().unwrap_or_default();
    match update["step_type"].as_str().unwrap_or_default() {
        "agent_response" => {
            let mut events = Vec::new();
            if let Some(text) = update["text_delta"]
                .as_str()
                .filter(|text| !text.is_empty())
            {
                events.push(AgentEvent::Delta {
                    text: text.to_string(),
                });
            }
            // The model call ends with its token counts (thinking tokens are output the model paid for).
            if state == "DONE" && update["usage"].is_object() {
                let usage = &update["usage"];
                let count = |key: &str| usage[key].as_u64().unwrap_or(0);
                events.push(AgentEvent::Usage {
                    input_tokens: count("input_tokens"),
                    output_tokens: count("output_tokens") + count("thinking_tokens"),
                    cache_read_tokens: count("cache_read_tokens"),
                    cache_write_tokens: 0,
                    new_call: true,
                });
            }
            events
        }
        // Some of agy's steps (a command left running, its status, input sent to it) carry another `step_type`
        // but still have a tool: they are shown like any other tool step instead of being dropped.
        kind if kind == "tool"
            || (kind != "agent_response"
                && (update["tool_name"].is_string() || update["tool_info"].is_object())) =>
        {
            let index = update["step_index"].as_u64().unwrap_or_default();
            let info = &update["tool_info"];
            let raw_name = update["tool_name"]
                .as_str()
                .or_else(|| info["name"].as_str())
                .unwrap_or("tool");
            let (name, input) = match raw_name {
                "call_mcp_tool" => mcp_call(&info["parameters"])
                    .unwrap_or_else(|| (raw_name.to_string(), tool_input(&info["parameters"]))),
                _ => (
                    tool_name(raw_name).to_string(),
                    tool_input(&info["parameters"]),
                ),
            };
            let parameters = &info["parameters"];
            let mut events = Vec::new();
            if name == "Bash" {
                // A refused command fails with a permission message, or ends without any output at all.
                let message = info["error"]["message"].as_str().unwrap_or_default();
                let refused = (state == "ERROR" && message.contains("permission"))
                    || (state == "DONE" && info["output"].is_null());
                if refused && !turn.refused.contains(&input) {
                    turn.refused.push(input.clone());
                }
            }
            if turn.started.insert(index) {
                let change = super::file_change(&name, parameters);
                if change.is_none() {
                    if let Some(snapshot) = super::begin_file_snapshot(
                        turn.root.as_deref(),
                        raw_name,
                        parameters,
                    ) {
                        turn.file_snapshots.insert(index, snapshot);
                    }
                }
                events.push(AgentEvent::ToolCall {
                    id: index.to_string(),
                    name,
                    input,
                    change,
                    background: is_subagent_tool(raw_name),
                });
                // agy runs a sub-agent in a conversation of its own and says nothing of what it does until it ends:
                // the card shows who it is from the start.
                if is_subagent_tool(raw_name) {
                    events.push(AgentEvent::Task {
                        id: index.to_string(),
                        tool_use_id: Some(index.to_string()),
                        status: TaskStatus::Started,
                        label: parameters["Role"].as_str().or_else(|| parameters["TypeName"].as_str()).map(str::to_owned),
                        summary: None,
                        tools: None,
                    });
                }
            }
            match state {
                "DONE" => {
                    // The step that ends a sub-agent lists who ran and what it was asked.
                    let listed = match &update["subagent_info"] {
                        Value::String(text) => serde_json::from_str::<Value>(text).unwrap_or(Value::Null),
                        other => other.clone(),
                    };
                    for agent in listed["subagents"].as_array().into_iter().flatten() {
                        events.push(AgentEvent::Task {
                            id: agent["conversation_id"].as_str().map_or_else(|| index.to_string(), str::to_owned),
                            tool_use_id: Some(index.to_string()),
                            status: TaskStatus::Done,
                            label: agent["role"].as_str().or_else(|| agent["type_name"].as_str()).map(str::to_owned),
                            summary: agent["initial_prompt"].as_str().map(|prompt| prompt.chars().take(200).collect()),
                            tools: None,
                        });
                    }
                    let output = info["output"].as_str().unwrap_or_default().to_string();
                    let image = offloaded_image(&output);
                    let change = turn
                        .file_snapshots
                        .remove(&index)
                        .and_then(super::finish_file_snapshot);
                    events.push(AgentEvent::ToolResult {
                        id: index.to_string(),
                        output,
                        is_error: false,
                        image,
                        change,
                    });
                }
                "ERROR" => {
                    turn.file_snapshots.remove(&index);
                    events.push(AgentEvent::ToolResult {
                        id: index.to_string(),
                        output: info["error"]["message"]
                            .as_str()
                            .unwrap_or("the tool failed")
                            .to_string(),
                        is_error: true,
                        image: None,
                        change: None,
                    });
                }
                _ => {}
            }
            events
        }
        // Any other step: kept for the Audit log once it has ended, so a step the chat misses can be looked at.
        // (The prompt and system steps carry nothing to see.)
        "user_input" | "system_message" => Vec::new(),
        kind if matches!(state, "DONE" | "ERROR") => vec![AgentEvent::Raw {
            source: "agy".into(),
            label: if kind.is_empty() {
                "step".into()
            } else {
                kind.to_string()
            },
            line: update.to_string(),
        }],
        _ => Vec::new(),
    }
}

/// The tools that start a sub-agent.
fn is_subagent_tool(name: &str) -> bool {
    matches!(name, "invoke_subagent" | "define_subagent" | "browser_subagent")
}

fn finish(result: &Value, turn: &Turn) -> Vec<AgentEvent> {
    if result["status"].as_str().unwrap_or("SUCCESS") != "SUCCESS" {
        // The error can list every model it would have taken: the first line says what went wrong.
        let message = result["error"]
            .as_str()
            .and_then(|text| text.lines().next())
            .unwrap_or("Antigravity failed");
        return vec![AgentEvent::Error {
            message: message.to_string(),
        }];
    }
    let mut events = Vec::new();
    // Only refused shell commands can be approved: `agy` names the action, and a command is the one kind whose
    // exact text we know (from its step) and can put in a rule.
    let asked = result["denied_actions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|action| action["action"] == "command");
    let denied: Vec<PermissionRequest> = if asked {
        turn.refused
            .iter()
            .filter(|command| ruleable(command))
            .enumerate()
            .map(|(index, command)| PermissionRequest {
                id: format!("denied-{index}"),
                tool: "Bash".to_string(),
                summary: command.clone(),
                allow: format!("{ALLOW_PREFIX}{command})"),
            })
            .collect()
    } else {
        Vec::new()
    };
    if !denied.is_empty() {
        events.push(AgentEvent::PermissionDenied { requests: denied });
    }
    events.push(AgentEvent::Done {
        session_id: result["conversation_id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .or_else(|| turn.conversation.clone()),
        text: result["response"].as_str().unwrap_or_default().to_string(),
        cost_usd: None,
    });
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(resume: bool) -> TurnRequest {
        TurnRequest {
            prompt: "hi".into(),
            session_id: Some("c1".into()),
            resume,
            model: Some("gemini-3.8-flash".into()),
            permission_mode: None,
            effort: Some("high".into()),
            allowed_tools: Vec::new(),
            cwd: None,
            owner: None,
            api_key: None,
        }
    }

    fn run(fixture: &str) -> Vec<AgentEvent> {
        let provider = Antigravity::default();
        fixture
            .lines()
            .flat_map(|line| provider.parse_line(line))
            .collect()
    }

    fn has(args: &[String], flag: &str) -> bool {
        args.iter().any(|arg| arg == flag)
    }

    fn after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|at| args.get(at + 1))
            .map(String::as_str)
    }

    #[test]
    fn a_new_conversation_passes_no_id_and_a_resumed_one_does() {
        let fresh = Antigravity::default().args(&req(false));
        assert!(!has(&fresh, "--conversation"));
        let resumed = Antigravity::default().args(&req(true));
        assert_eq!(after(&resumed, "--conversation"), Some("c1"));
        assert_eq!(after(&resumed, "--output-format"), Some("stream-json"));
        assert_eq!(after(&resumed, "-p"), Some("hi"));
    }

    #[test]
    fn only_the_first_message_of_a_conversation_carries_the_opening_context() {
        let fresh = Antigravity::default().args(&req(false));
        let opening = after(&fresh, "-p").unwrap_or_default();
        assert!(opening.contains("```agentdeck-ask"));
        assert!(opening.ends_with("\n\nhi"));
        let resumed = Antigravity::default().args(&req(true));
        assert_eq!(after(&resumed, "-p"), Some("hi"));
    }

    #[test]
    fn effort_goes_in_its_own_flag_unless_the_model_id_has_it() {
        let args = Antigravity::default().args(&req(false));
        assert_eq!(
            (after(&args, "--model"), after(&args, "--effort")),
            (Some("gemini-3.8-flash"), Some("high"))
        );
        let mut suffixed = req(false);
        suffixed.model = Some("gemini-3.8-flash-low".into());
        assert!(!has(&Antigravity::default().args(&suffixed), "--effort"));
    }

    #[test]
    fn permission_modes_map_to_agy_modes() {
        let args_for = |mode: &str| {
            let mut request = req(false);
            request.permission_mode = Some(mode.into());
            Antigravity::default().args(&request)
        };
        assert_eq!(
            after(&args_for("acceptEdits"), "--mode"),
            Some("accept-edits")
        );
        assert_eq!(after(&args_for("plan"), "--mode"), Some("plan"));
        assert_eq!(after(&args_for("auto"), "--mode"), Some("auto"));
        assert!(has(
            &args_for("bypassPermissions"),
            "--dangerously-skip-permissions"
        ));
        let default = args_for("default");
        assert!(!has(&default, "--mode") && !has(&default, "--dangerously-skip-permissions"));
    }

    #[test]
    fn approving_a_command_never_turns_off_the_prompts() {
        let mut request = req(true);
        request.allowed_tools = vec!["agy:command(echo hi)".into()];
        assert!(!has(
            &Antigravity::default().args(&request),
            "--dangerously-skip-permissions"
        ));
    }

    #[test]
    fn an_approval_becomes_one_exact_rule_and_only_a_safe_one() {
        assert_eq!(
            allow_rule("agy:command(echo hi)").as_deref(),
            Some("command(echo hi)")
        );
        assert_eq!(
            allow_rule("agy:command(git commit -m \"x\")").as_deref(),
            Some("command(git commit -m \"x\")")
        );
        // Not ours, empty, or able to end the rule early or span lines: no rule.
        for refused in [
            "Bash(git status)",
            "agy:command()",
            "agy:command(  )",
            "agy:command(echo (a))",
            "agy:command(a\nb)",
            "agy:command(echo a",
            "agy:command",
        ] {
            assert_eq!(allow_rule(refused), None, "{refused}");
        }
    }

    fn settings_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentdeck-agy-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn rules_are_added_once_and_the_rest_of_the_settings_stay() {
        let path = settings_path("merge");
        std::fs::write(&path, r#"{"trustedWorkspaces":["C:\\Users\\u"],"statusLine":{"enabled":true},"permissions":{"allow":["command(ls)"]}}"#).unwrap();
        let rules = vec!["command(echo hi)".to_string(), "command(ls)".to_string()];
        add_allow_rules(&path, &rules).unwrap();
        add_allow_rules(&path, &rules).unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            saved["permissions"]["allow"],
            serde_json::json!(["command(ls)", "command(echo hi)"])
        );
        assert_eq!(saved["statusLine"]["enabled"], true);
        // The key order of the file is kept, and the original was copied once.
        let keys: Vec<&String> = saved.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["trustedWorkspaces", "statusLine", "permissions"]);
        let backup = std::fs::read_to_string(path.with_extension("json.agentdeck.bak")).unwrap();
        assert!(!backup.contains("echo hi"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_missing_settings_file_is_created_and_a_broken_one_is_left_alone() {
        let path = settings_path("create");
        add_allow_rules(&path, &["command(echo hi)".to_string()]).unwrap();
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            saved["permissions"]["allow"],
            serde_json::json!(["command(echo hi)"])
        );
        std::fs::write(&path, "{ not json").unwrap();
        assert!(add_allow_rules(&path, &["command(ls)".to_string()]).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_plain_answer_streams_text_then_finishes() {
        let events = run(include_str!("../../tests/fixtures/agy/plain.jsonl"));
        assert!(matches!(events.first(), Some(AgentEvent::Delta { .. })));
        let text: String = events
            .iter()
            .filter_map(|event| {
                if let AgentEvent::Delta { text } = event {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(!text.trim().is_empty());
        assert!(events.iter().any(|event| matches!(event, AgentEvent::Usage { input_tokens, new_call: true, .. } if *input_tokens > 0)));
        let Some(AgentEvent::Done { session_id, .. }) = events.last() else {
            panic!("no Done: {events:?}")
        };
        assert!(session_id.as_deref().is_some_and(|id| !id.is_empty()));
    }

    #[test]
    fn tool_steps_become_a_call_and_a_result_once() {
        let events = run(include_str!(
            "../../tests/fixtures/agy/edits_and_denied.jsonl"
        ));
        let calls: Vec<(&str, &str)> = events
            .iter()
            .filter_map(|event| {
                if let AgentEvent::ToolCall { name, input, .. } = event {
                    Some((name.as_str(), input.as_str()))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(
            calls.iter().map(|call| call.0).collect::<Vec<_>>(),
            ["Write", "Edit", "Bash"]
        );
        assert!(calls[0].1.ends_with("b.txt"));
        let results = events
            .iter()
            .filter(|event| matches!(event, AgentEvent::ToolResult { .. }))
            .count();
        assert_eq!(results, 3);
    }


    #[test]
    fn a_sub_agent_is_a_background_call_with_a_task_that_names_it() {
        let events = run(include_str!("../../tests/fixtures/agy/subagent.jsonl"));
        let call = events.iter().find_map(|event| match event {
            AgentEvent::ToolCall { id, background, .. } if *background => Some(id.clone()),
            _ => None,
        });
        let call = call.expect("the invoke_subagent call is marked as background work");
        let tasks: Vec<&AgentEvent> = events.iter().filter(|event| matches!(event, AgentEvent::Task { .. })).collect();
        assert!(matches!(tasks.first(), Some(AgentEvent::Task { status: TaskStatus::Started, tool_use_id: Some(id), label: Some(label), .. }) if *id == call && label == "Math Calculator"), "{tasks:?}");
        assert!(tasks.iter().any(|event| matches!(event, AgentEvent::Task { status: TaskStatus::Done, summary: Some(summary), .. } if summary.contains("17*3"))), "{tasks:?}");
        assert!(matches!(events.last(), Some(AgentEvent::Done { .. })));
    }
    #[test]
    fn a_denied_command_asks_for_approval_before_done() {
        let events = run(include_str!("../../tests/fixtures/agy/tool_denied.jsonl"));
        let at = events
            .iter()
            .position(|event| matches!(event, AgentEvent::PermissionDenied { .. }))
            .expect("a permission request");
        let AgentEvent::PermissionDenied { requests } = &events[at] else {
            unreachable!()
        };
        assert_eq!(requests.len(), 1);
        assert_eq!(
            (
                requests[0].tool.as_str(),
                requests[0].summary.as_str(),
                requests[0].allow.as_str()
            ),
            ("Bash", "echo hi", "agy:command(echo hi)")
        );
        assert!(matches!(events[at + 1], AgentEvent::Done { .. }));
        // The refused tool failed with the reason the CLI gave.
        assert!(events.iter().any(|event| matches!(event, AgentEvent::ToolResult { is_error: true, output, .. } if output.contains("permission"))));
    }

    #[test]
    fn a_command_that_ended_without_output_is_a_refusal_too() {
        let events = run(include_str!(
            "../../tests/fixtures/agy/edits_and_denied.jsonl"
        ));
        let Some(AgentEvent::PermissionDenied { requests }) = events
            .iter()
            .find(|event| matches!(event, AgentEvent::PermissionDenied { .. }))
        else {
            panic!("no request: {events:?}")
        };
        assert!(requests[0].allow.starts_with("agy:command(Select-String"));
    }

    #[test]
    fn a_refused_command_that_cannot_be_ruled_gives_no_request() {
        let provider = Antigravity::default();
        let lines = [
            r#"{"event":"step_update","step_update":{"step_index":2,"state":"ERROR","step_type":"tool","tool_name":"run_command","tool_info":{"name":"run_command","parameters":{"CommandLine":"echo (a)"},"error":{"message":"permission check failed"}}}}"#,
            r#"{"event":"result","result":{"conversation_id":"c","status":"SUCCESS","response":"","denied_actions":[{"action":"command","display_name":"RunCommand"}]}}"#,
        ];
        let events: Vec<AgentEvent> = lines
            .iter()
            .flat_map(|line| provider.parse_line(line))
            .collect();
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, AgentEvent::PermissionDenied { .. })),
            "{events:?}"
        );
        assert!(matches!(events.last(), Some(AgentEvent::Done { .. })));
    }

    #[test]
    fn a_resumed_turn_keeps_the_conversation_id() {
        let events = run(include_str!("../../tests/fixtures/agy/resumed.jsonl"));
        let Some(AgentEvent::Done {
            session_id, text, ..
        }) = events.last()
        else {
            panic!("no Done: {events:?}")
        };
        assert_eq!(
            session_id.as_deref(),
            Some("d9c2144e-aeae-4bbd-897b-3871fe7122da")
        );
        assert_eq!(text.trim(), "b.txt");
    }

    #[test]
    fn a_failed_run_reports_only_the_first_line_of_its_error() {
        let events = run(include_str!("../../tests/fixtures/agy/bad_model.jsonl"));
        let [AgentEvent::Error { message }] = events.as_slice() else {
            panic!("expected one error: {events:?}")
        };
        assert!(message.starts_with("invalid model selection"));
        assert!(!message.contains('\n'));
    }

    #[test]
    fn lines_it_does_not_know_give_nothing() {
        let provider = Antigravity::default();
        assert!(provider.parse_line("not json").is_empty());
        assert!(provider
            .parse_line(r#"{"event":"something_new"}"#)
            .is_empty());
        assert!(provider.parse_line(r#"{"event":"step_update","step_update":{"step_type":"system_message","state":"DONE"}}"#).is_empty());
        // A step of a kind it does not know is only kept, as written, for the Audit log, once it has ended.
        let line =
            r#"{"event":"step_update","step_update":{"step_type":"something_new","state":"DONE"}}"#;
        assert!(
            matches!(provider.parse_line(line).as_slice(), [AgentEvent::Raw { label, .. }] if label == "something_new")
        );
        assert!(provider.parse_line(r#"{"event":"step_update","step_update":{"step_type":"something_new","state":"ACTIVE"}}"#).is_empty());
    }

    #[test]
    fn the_binary_is_looked_up_in_its_own_install_folder() {
        let found = candidates_for("windows", None, Some(PathBuf::from("C:/Local")));
        assert_eq!(
            found,
            [PathBuf::from("C:/Local")
                .join("agy")
                .join("bin")
                .join("agy.exe")]
        );
        let found = candidates_for("linux", Some(PathBuf::from("/home/u")), None);
        assert_eq!(found, [PathBuf::from("/home/u/.local/bin/agy")]);
    }

    #[test]
    fn a_picture_a_tool_saved_becomes_a_thumbnail_only_from_agys_own_folder() {
        let home = std::env::temp_dir().join(format!("agentdeck-agy-img-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let shots = home
            .join(".gemini")
            .join("antigravity-cli")
            .join("brain")
            .join("c 1")
            .join("media_0.png");
        std::fs::create_dir_all(shots.parent().expect("has a parent")).expect("makes folders");
        std::fs::write(&shots, [137u8, 80, 78, 71]).expect("writes");
        std::fs::write(home.join("secret.png"), [1u8, 2, 3]).expect("writes");
        let slashes = shots
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/")
            .replace(' ', "%20");
        let address = if slashes.starts_with('/') {
            format!("file://{slashes}")
        } else {
            format!("file:///{slashes}")
        };
        let output = format!("{{}}\n[Resource offloaded to {address}]");
        let image = offloaded_image_in(&output, &home).expect("a thumbnail");
        assert!(image.starts_with("data:image/png;base64,iVBORw"), "{image}");
        let outside = format!(
            "[Resource offloaded to file:///{}]",
            home.join("secret.png")
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/")
        );
        assert_eq!(
            offloaded_image_in(&outside, &home),
            None,
            "only files under .gemini are read"
        );
        assert_eq!(offloaded_image_in("no picture here", &home), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn an_mcp_call_is_named_like_claude_names_it() {
        let call = serde_json::json!({ "ServerName": "agentdeck", "ToolName": "browser_click", "Arguments": { "x": 90, "y": 513 } });
        let (name, input) = mcp_call(&call).expect("a call");
        assert_eq!(name, "mcp__agentdeck__browser_click");
        assert!(input.contains("\"x\":90"), "{input}");
        assert_eq!(
            mcp_call(&serde_json::json!({ "ServerName": "agentdeck" })),
            None
        );
        assert_eq!(percent_decode("a%20b%2Fc%zz"), "a b/c%zz");
    }
}
