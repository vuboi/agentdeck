//! Chat with a custom provider (any OpenAI-compatible server, or Ollama through its `/v1` route).
//!
//! There is no CLI to run, so this is the agent loop itself: one streamed `chat/completions` request per step,
//! each tool the model calls is run here and its result sent back, until the model answers without a tool.
//! Tools: Bash, Read, Write and Edit (named like Claude Code's, so the chat draws them the same way), plus the
//! AgentDeck browser MCP tools. Every event is an [`AgentEvent`], the same as the CLI agents produce.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::mcp_client::Server;
use crate::slash_commands::{self, ProviderCommand};
use ag_adapters::agent::warm::InjectSlot;
use ag_adapters::agent::{allow_entry, summarize_tool_input, AgentEvent, PermissionRequest};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::process::Command;

/// Model calls in one turn before it is stopped, so a model that keeps calling tools cannot run forever.
const MAX_STEPS: usize = 40;
/// A local model that repeats the same tool call is not making progress. Stop the turn early instead of
/// spending the remaining requests until `MAX_STEPS`.
const REPEATED_CALL_LIMIT: usize = 3;
/// Web searches refused with 429 in one turn before the turn stops.
const SEARCH_LIMIT_HITS: usize = 3;
/// Give a tool-heavy model a chance to hand back its result before the hard stop.
const FINAL_NUDGE_AT: usize = 30;
/// Characters of a tool's output kept for the model.
const OUTPUT_MAX: usize = 12_000;
/// How long the model may be silent in the middle of an answer. A big local model can take a while to start.
const STALL: Duration = Duration::from_secs(240);
/// How long one browser or MCP tool call may take.
const TOOL_LIMIT: Duration = Duration::from_secs(150);
const MCP_PREFIX: &str = "mcp__agentdeck__";

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// Pictures that go with the message, as data URLs.
    #[serde(default)]
    pub images: Vec<String>,
    /// Documents that go with the message, as paths: text is put into the message, anything else is named.
    #[serde(default)]
    pub files: Vec<String>,
}

/// Characters of one attached text file that go into the message.
const FILE_INLINE_MAX: usize = 40_000;
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "mdx", "log", "csv", "tsv", "json", "yaml", "yml", "toml", "xml", "html", "css",
    "scss", "js", "ts", "tsx", "jsx", "svelte", "vue", "py", "rs", "go", "java", "c", "cpp", "h",
    "cs", "rb", "php", "sh", "ps1", "sql", "ini", "env",
];

/// A message as the provider takes it: the text with the attached documents after it, and the pictures in the
/// OpenAI image format when there are any.
fn wire_message(message: &ChatMessage, text_only: bool) -> Value {
    let mut text = message.content.clone();
    for path in &message.files {
        let file = std::path::Path::new(path);
        let name = file
            .file_name()
            .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
        let extension = file
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let inline = if TEXT_EXTENSIONS.contains(&extension.as_str()) {
            std::fs::read_to_string(file).ok()
        } else {
            None
        };
        match inline {
            Some(body) => {
                let cut: String = body.chars().take(FILE_INLINE_MAX).collect();
                let more = if cut.len() < body.len() {
                    "\n[...cut: the file is longer]"
                } else {
                    ""
                };
                text.push_str(&format!(
                    "\n\nAttached file {name} ({path}):\n```\n{cut}{more}\n```"
                ));
            }
            None => text.push_str(&format!(
                "\n\nAttached file {name}, at {path} (open it with a tool if you need it)."
            )),
        }
    }
    // Workers AI takes `content` as a string only, so a text-only API gets the pictures named rather than sent.
    if text_only && !message.images.is_empty() {
        text.push_str(&format!(
            "\n\n[{} image(s) attached: this model reads text only]",
            message.images.len()
        ));
    }
    if message.images.is_empty() || text_only {
        return json!({ "role": message.role, "content": text });
    }
    let mut parts = vec![json!({ "type": "text", "text": text })];
    parts.extend(
        message
            .images
            .iter()
            .map(|url| json!({ "type": "image_url", "image_url": { "url": url } })),
    );
    json!({ "role": message.role, "content": parts })
}

/// Everything about one turn that is not the conversation.
pub struct Turn {
    pub kind: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub key_env: Option<String>,
    pub model: String,
    pub cwd: Option<PathBuf>,
    /// The CLI-style permission mode (`acceptEdits`, `plan`, `auto`, `bypassPermissions`); `None` asks for every tool.
    pub permission_mode: Option<String>,
    /// Tools the user already approved, as `Write` or `Bash(git status)`.
    pub allowed: Vec<String>,
    /// The chat, which tags the browser tools the model calls.
    pub chat: String,
    /// How much the model thinks: `off`, or a level. `None` leaves the model's own default.
    pub effort: Option<String>,
    /// A provider whose free tier may keep and train on what it is sent: it gets no file, shell, browser, code index or
    /// MCP tool and no project text, only web search and fetch. A trusted model reads the project and hands it text.
    pub restricted: bool,
    /// WorkBuddy is OpenAI-shaped but requires its own gateway headers and prompt normalization.
    pub workbuddy: bool,
}

/// The only tools a restricted provider may call: they read the public web, never this machine.
const RESTRICTED_TOOLS: [&str; 3] = ["WebSearch", "WebFetch", "PackageInfo"];

const RESTRICTED_PROMPT: &str = "You are an assistant inside AgentDeck for light work: searching and reading public information, writing emails, prompts and scripts, explaining, brainstorming. You have web tools only (WebSearch, WebFetch, PackageInfo). You cannot see the user's files, project or machine and must not ask for them. If the work needs the project's files, code, credentials or anything private, say so in one line: a different agent has to do that part. Never ask for, repeat or store secrets (keys, passwords, tokens, environment variables, internal IPs or host names); AgentDeck blocks any message or tool call that holds one. Call tools for real: never pretend a tool ran. Answer in the user's language.";

/// What in a chat must not go to a restricted provider: a file that holds secrets by its name, or a secret, an
/// environment variable or an internal address in a message or an attached text file. `None`: the chat may be sent.
fn restricted_leak(history: &[ChatMessage]) -> Option<String> {
    for message in history {
        if let Some(what) = crate::privacy::private_content(&message.content) {
            return Some(format!("the message holds {what}"));
        }
        for path in &message.files {
            let name = std::path::Path::new(path)
                .file_name()
                .map_or_else(|| path.clone(), |name| name.to_string_lossy().into_owned());
            if crate::privacy::sensitive_file(path) {
                return Some(format!("{name} is a file that holds secrets"));
            }
            let what = std::fs::read_to_string(path)
                .ok()
                .and_then(|text| crate::privacy::private_content(&text));
            if let Some(what) = what {
                return Some(format!("{name} holds {what}"));
            }
        }
    }
    None
}

/// The address of the chat route for a provider kind.
pub(crate) fn chat_url(kind: &str, base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    match kind {
        "ollama" => format!("{base}/v1/chat/completions"),
        _ => format!("{base}/chat/completions"),
    }
}

pub struct PrehandleRequest {
    pub kind: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub prompt: String,
    pub images: Vec<String>,
    pub files: Vec<String>,
}

/// The instructions of a pre-handle pass with the user's message and the text of any attached text files.
async fn memo_prompt(prompt: &str, files: &[String]) -> String {
    let mut text = format!(
        "You are AgentDeck's attachment pre-handler. Extract only useful facts from the attached images/files for another coding agent. \
Return a compact plain-text memo with these headings: Visible facts, Text/OCR, User intent, Uncertainty. \
Do not invent details, do not repeat secrets, and keep it under 1200 words. User message:\n{}",
        prompt.trim()
    );
    for path in files.iter().take(8) {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        if crate::privacy::sensitive_file(path) {
            text.push_str(&format!(
                "\n\nAttached file {name}: [content omitted because it may contain secrets]"
            ));
            continue;
        }
        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if TEXT_EXTENSIONS.contains(&extension.as_str()) {
            if let Ok(body) = tokio::fs::read_to_string(path).await {
                let cut: String = body.chars().take(12_000).collect();
                text.push_str(&format!("\n\nAttached text file {name}:\n{cut}"));
            } else {
                text.push_str(&format!("\n\nAttached file: {name}"));
            }
        } else {
            text.push_str(&format!("\n\nAttached file: {name}"));
        }
    }
    text
}

/// The same memo, read by a free OpenCode model through the OpenCode CLI: the pictures go in as `--file`, the prompt on
/// stdin. Standalone, so the pass does not share the user's OpenCode session. The JSON event stream's text parts are the memo.
pub async fn prehandle_opencode(
    model: String,
    prompt: String,
    images: Vec<String>,
    files: Vec<String>,
) -> Result<String, String> {
    if images.is_empty() && files.is_empty() {
        return Ok(String::new());
    }
    use tokio::io::AsyncWriteExt;
    let binary = ag_adapters::agent::resolve::resolve_binary(
        &ag_adapters::agent::opencode::OpenCode::default(),
    );
    let mut command = Command::new(binary);
    command.args(["run", "--standalone", "--format", "json", "--model", &model]);
    for path in images.iter().take(4) {
        command.args(["--file", path]);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start OpenCode: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(memo_prompt(&prompt, &files).await.as_bytes())
            .await
            .map_err(|error| format!("could not send the pre-handle prompt: {error}"))?;
    }
    let output = tokio::time::timeout(Duration::from_secs(120), child.wait_with_output())
        .await
        .map_err(|_| "OpenCode did not answer in time".to_owned())?
        .map_err(|error| format!("OpenCode failed: {error}"))?;
    // The CLI reports a failed call (expired key, no quota) as an `error` event on stdout, not on stderr.
    let events: Vec<Value> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect();
    if let Some(error) = events.iter().find(|event| event["type"] == "error") {
        let message: String = error["error"]["message"]
            .as_str()
            .unwrap_or("unknown error")
            .chars()
            .take(200)
            .collect();
        return Err(format!(
            "OpenCode could not read the attachments: {message}"
        ));
    }
    if !output.status.success() {
        return Err("OpenCode could not read the attachments".to_owned());
    }
    let memo: String = events
        .iter()
        .filter(|event| event["type"] == "text")
        .filter_map(|event| event["part"]["text"].as_str().map(str::to_owned))
        .collect::<Vec<_>>()
        .join("");
    let memo = memo.trim().to_owned();
    if memo.is_empty() {
        Err("pre-handler returned an empty memo".to_owned())
    } else {
        Ok(memo)
    }
}

/// The same memo, read by Antigravity's CLI in print mode. That CLI runs as an agent, so it reads the pictures with its own
/// tools: they are named in the prompt and their folders are opened with `--add-dir`. Its permissions are skipped (the full
/// options the user chose), with `--sandbox` on so its terminal stays restricted. The prompt is an argument, so it is capped.
pub async fn prehandle_antigravity(
    model: String,
    prompt: String,
    images: Vec<String>,
    files: Vec<String>,
) -> Result<String, String> {
    if images.is_empty() && files.is_empty() {
        return Ok(String::new());
    }
    let binary = ag_adapters::agent::resolve::resolve_binary(
        &ag_adapters::agent::antigravity::Antigravity::default(),
    );
    let mut text = memo_prompt(&prompt, &files).await;
    let pictures: Vec<&String> = images.iter().take(4).collect();
    if !pictures.is_empty() {
        let names = pictures
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        text.push_str(&format!(
            "\n\nAttached pictures, open each with your file tools and describe it:\n{names}"
        ));
    }
    let text: String = text.chars().take(24_000).collect();
    let mut command = Command::new(binary);
    command.arg(format!("-p={text}"));
    command.args([
        "--model",
        &model,
        "--effort",
        "low",
        "--dangerously-skip-permissions",
        "--sandbox",
    ]);
    for path in &pictures {
        let folder = std::path::Path::new(path.as_str())
            .parent()
            .map(|folder| folder.to_string_lossy().into_owned())
            .unwrap_or_default();
        command.args(["--add-dir", &folder]);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let child = command
        .spawn()
        .map_err(|error| format!("could not start Antigravity: {error}"))?;
    let output = tokio::time::timeout(Duration::from_secs(120), child.wait_with_output())
        .await
        .map_err(|_| "Antigravity did not answer in time".to_owned())?
        .map_err(|error| format!("Antigravity failed: {error}"))?;
    if !output.status.success() {
        return Err("Antigravity could not read the attachments".to_owned());
    }
    let memo = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if memo.is_empty() {
        Err("pre-handler returned an empty memo".to_owned())
    } else {
        Ok(memo)
    }
}

/// Runs a short, stateless vision pass. It deliberately has no tools or chat history: the result is only a compact memo
/// that the main agent can use instead of paying to ingest the original pictures on every provider.
pub async fn prehandle_attachments(request: PrehandleRequest) -> Result<String, String> {
    if request.images.is_empty() && request.files.is_empty() {
        return Ok(String::new());
    }
    // Anthropic's Messages API takes pictures as `source` blocks, not as data URLs.
    let anthropic = request.kind == "anthropic";

    use base64::Engine;
    let mut parts = Vec::new();
    let mut image_count = 0usize;
    for path in request.images.iter().take(4) {
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|error| format!("could not read attachment: {error}"))?;
        if bytes.len() > 8 * 1024 * 1024 {
            return Err("an attached image is larger than 8 MB".to_owned());
        }
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("png")
            .to_ascii_lowercase();
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "png" => "image/png",
            _ => "application/octet-stream",
        };
        if mime == "application/octet-stream" {
            continue;
        }
        image_count += 1;
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        parts.push(if anthropic {
            json!({ "type": "image", "source": { "type": "base64", "media_type": mime, "data": data } })
        } else {
            json!({ "type": "image_url", "image_url": { "url": format!("data:{mime};base64,{data}") } })
        });
    }

    if image_count == 0 && request.files.is_empty() {
        return Ok(String::new());
    }
    let text = memo_prompt(&request.prompt, &request.files).await;
    parts.insert(0, json!({ "type": "text", "text": text }));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| format!("could not create pre-handler client: {error}"))?;
    let mut builder = if anthropic {
        // A base typed with its version (`https://api.anthropic.com/v1`) is the same address, as in `models_url`.
        let base = request.base_url.trim().trim_end_matches('/');
        client
            .post(format!("{}/v1/messages", base.trim_end_matches("/v1")))
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": request.model,
                "messages": [{ "role": "user", "content": parts }],
                "temperature": 0.1,
                "max_tokens": 1400,
                // The cheapest effort: a memo of what the picture shows needs no deep thinking.
                "output_config": { "effort": "low" }
            }))
    } else {
        client
            .post(chat_url(&request.kind, &request.base_url))
            .json(&json!({
                "model": request.model,
                "messages": [{ "role": "user", "content": parts }],
                "temperature": 0.1,
                "max_tokens": 1400,
                "reasoning_effort": "low",
                "stream": false
            }))
    };
    // Copilot takes the signed-in `gh` token, the same as its model list, and the headers its clients send.
    let copilot = request.kind == "copilot";
    let key = if copilot {
        Some(crate::copilot::github_token().await?)
    } else {
        request.api_key
    };
    if copilot {
        builder = builder
            .header("Copilot-Integration-Id", "vscode-chat")
            .header("Editor-Version", "vscode/1.99.0");
    }
    if let Some(key) = key.filter(|value| !value.trim().is_empty()) {
        builder = if anthropic {
            builder.header("x-api-key", key)
        } else {
            builder.bearer_auth(key)
        };
    }
    let response = builder
        .send()
        .await
        .map_err(|error| format!("pre-handler request failed: {error}"))?;
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|error| format!("invalid pre-handler response: {error}"))?;
    if !status.is_success() {
        return Err(value["error"]["message"]
            .as_str()
            .unwrap_or("pre-handler provider returned an error")
            .to_owned());
    }
    let answer = if anthropic {
        &value["content"]
    } else {
        &value["choices"][0]["message"]["content"]
    };
    let content = answer
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            answer.as_array().map(|parts| {
                parts
                    .iter()
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("")
            })
        })
        .unwrap_or_default()
        .trim()
        .to_owned();
    if content.is_empty() {
        Err("pre-handler returned an empty memo".to_owned())
    } else {
        Ok(content)
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Tools

fn function(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({ "type": "function", "function": { "name": name, "description": description, "parameters": { "type": "object", "properties": properties, "required": required } } })
}

fn builtin_tools() -> Vec<Value> {
    vec![
        function("Bash", "Runs a shell command in the project folder and returns its output. Do not start dev servers with it: use browser_serve.", json!({ "command": { "type": "string", "description": "the command to run" } }), &["command"]),
        function("Read", "Reads a text file and returns it with line numbers.", json!({ "file_path": { "type": "string" }, "offset": { "type": "number", "description": "first line to read (1-based)" }, "limit": { "type": "number", "description": "how many lines" } }), &["file_path"]),
        function("Write", "Creates or overwrites a file with the given content.", json!({ "file_path": { "type": "string" }, "content": { "type": "string" } }), &["file_path", "content"]),
        function("Edit", "Replaces one exact piece of text in a file. old_string must appear exactly once, unless replace_all is true.", json!({ "file_path": { "type": "string" }, "old_string": { "type": "string" }, "new_string": { "type": "string" }, "replace_all": { "type": "boolean", "description": "replace every occurrence" } }), &["file_path", "old_string", "new_string"]),
        function("LS", "Lists the files and folders of a folder.", json!({ "path": { "type": "string", "description": "folder (default: the project folder)" } }), &[]),
        function("MkDir", "Creates a folder, with any missing parent folders.", json!({ "path": { "type": "string" } }), &["path"]),
        function("Move", "Moves or renames a file or folder. Fails if the destination exists.", json!({ "from": { "type": "string" }, "to": { "type": "string" } }), &["from", "to"]),
        function("Copy", "Copies a file or folder. Fails if the destination exists.", json!({ "from": { "type": "string" }, "to": { "type": "string" } }), &["from", "to"]),
        function("Delete", "Deletes a file or folder inside the project folder.", json!({ "path": { "type": "string" } }), &["path"]),
        function("WebSearch", "Searches the web and returns the first results (title, address, snippet). Follow up with WebFetch to read a page.", json!({ "query": { "type": "string" } }), &["query"]),
        function("Glob", "Finds files by name pattern (for example src/**/*.svelte or *.json). Returns paths relative to the project folder.", json!({ "pattern": { "type": "string" }, "path": { "type": "string", "description": "folder to search (default: the project folder)" } }), &["pattern"]),
        function("Grep", "Searches file contents with a regular expression. Returns path:line: text.", json!({ "pattern": { "type": "string", "description": "regular expression" }, "path": { "type": "string", "description": "folder or file (default: the project folder)" }, "glob": { "type": "string", "description": "only files matching this name pattern, e.g. *.ts" }, "ignore_case": { "type": "boolean" } }), &["pattern"]),
        function("Plan", "Records your plan before you change anything: the files you will change and the steps. Edit, Write, Move and Delete refuse to run until this was called, and Edit and Write only touch the files listed here. Call it again with the full list if you find another file must change.", json!({ "goal": { "type": "string", "description": "what the user wants, in one sentence" }, "files": { "type": "array", "items": { "type": "string" }, "description": "paths of the files you will change or create, found with code_search, Grep or Glob" }, "steps": { "type": "array", "items": { "type": "string" }, "description": "the edits, in order, each saying where" }, "checks": { "type": "string", "description": "how you will check the result (a build, lint or test command, or a look at the Preview)" } }), &["goal", "files", "steps"]),
        function("PackageInfo", "The latest version of a package, with its description, home page and whether it is deprecated, from the package registry (npm, crates.io or PyPI). Use it before writing code against a library, and compare with the version the project has.", json!({ "name": { "type": "string" }, "ecosystem": { "type": "string", "description": "npm (default), crates or pypi" } }), &["name"]),
        function("WebFetch", "Downloads a web page or API address and returns its text (HTML is reduced to its text).", json!({ "url": { "type": "string" } }), &["url"]),
        function("Skill", "Loads the full instructions of one of the skills listed in the system message. Call it when the task matches a skill, then follow what it says.", json!({ "name": { "type": "string", "description": "the skill's name" } }), &["name"]),
    ]
}

/// The AgentDeck browser tools, asked of the MCP server the app runs for itself, as OpenAI functions.
async fn browser_tools(app: &AppHandle, chat: &str) -> Vec<Value> {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }).to_string();
    let Some(answer) = crate::mcp::handle(app, &request, chat).await else {
        return Vec::new();
    };
    let Ok(answer) = serde_json::from_str::<Value>(&answer) else {
        return Vec::new();
    };
    answer["result"]["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|tool| {
            function(
                tool["name"].as_str().unwrap_or_default(),
                tool["description"].as_str().unwrap_or_default(),
                tool["inputSchema"]["properties"].clone(),
                &tool["inputSchema"]["required"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// What running a tool gave: the text for the model, whether it failed, and a picture it returned.
mod tools;
pub(super) use tools::{
    browser_call, change_for, decide, delete_path, edit_file, failed, glob_files, grep_files,
    list_dir, make_dir, ok, package_info, path_key, plan_tool, project_brief, read_file, resolve,
    run_bash, transfer, trim_output, web_fetch, web_search, write_file, Decision, Outcome,
    BALANCED, START_HEAVY,
};

mod protocol;
pub(crate) use protocol::{model_call, Call};

#[cfg(test)]
pub(crate) use protocol::{
    handle_sse_line, messages_provider, read_chunk, read_messages_chunk, read_responses_chunk,
    responses_input, responses_provider, take_line, Step,
};

#[cfg(test)]
pub(crate) use tools::{bing_results, brave_results, duckduckgo_results, END_HEAVY};

fn system_prompt(cwd: Option<&Path>, skills: &[ProviderCommand]) -> String {
    let folder = cwd
        .map(|dir| format!(" The working folder is {}.", dir.display()))
        .unwrap_or_default();
    let mut prompt = format!(
        "You are a coding agent running inside AgentDeck, a desktop app.{folder} You work through tools, and you only know what they show you: never invent a file, a function, an API or a result. \
         Tools. Code index (fast, use it first to find things): code_search (a symbol by name), code_outline (the structure of a file), code_definition (a symbol with its source), code_references (where a name is used), code_imports and code_importers (what a file needs, and what breaks if you change it). \
         Files: LS, Glob (by name), Grep (by content), Read, Write, Edit, MkDir, Move (also renames), Copy, Delete. Shell: Bash (git, build, tests, installs). Knowledge: WebSearch then WebFetch (official docs, release notes), PackageInfo (latest version, deprecated or not). Plan. Skill. \
         Call tools for real: never describe a tool call in text or pretend it ran. \
         HOW YOU WORK on any task that changes files, in this order, one phase at a time. \
         1 LOCATE. Find where the change belongs before anything else: code_search or code_outline for a name, Grep for a text or a class from the page, code_importers before changing something other files use. Read the files you found, around the place that matters. If a search finds nothing, try other words (a parent's name, the visible text, a component's selector) before guessing. \
         2 CHECK CURRENT PRACTICE. Your memory of libraries is old. Read the versions the project uses (given below, or package.json, Cargo.toml). For any library or framework API you are not certain exists in that version, WebSearch for it with the version (for example \"angular 20 signal inputs\") and WebFetch the official documentation, or PackageInfo for the latest version and whether it is deprecated. Prefer what the project's own code already does: look at a neighbouring file doing something similar and copy its pattern, imports and components. Do not use APIs the docs mark deprecated. \
         3 PLAN. Call Plan with the goal, the exact files you will change (the ones you found, not guesses), the steps and how you will check. Edit and Write refuse to run without it, and only touch the files in it. \
         4 EDIT. One small change at a time with Edit (Read the file first; old_string copied exactly from what Read showed). Put it exactly where the request says, written like the code around it. Never rewrite a whole file for a small change and never create something elsewhere because it is easier. \
         5 VERIFY. Run the project's own check with Bash (build, lint, type check, tests: see its scripts), read the errors and fix them, check with code_references that nothing that used a changed name broke, and if the Preview is open look at the page. Only then answer, saying what changed and in which files, and what you checked. \
         A question or an explanation needs no plan: answer from what the tools show. \
         Browser: the browser_* tools drive the Preview panel, the page the user is looking at. To run a dev server use browser_serve (a server started with Bash dies when the command ends), then browser_open if needed. When the user asks you to run a project or its source (also \"chạy project\", \"chạy source\"), that is a dev server even when no page is named: find its start command from the project's own scripts or launch config (.claude/launch.json, a Makefile, a task runner), or from its build or manifest files and README for the language and toolchain, whatever it is; then start it with browser_serve. \
         To act on a page, call browser_elements for the list of buttons, links and fields with their numbers, then browser_click_element with a number (or the visible text, or a CSS selector); use browser_fill to fill a field by selector. Do not guess coordinates. \
         A dialog, drawer or menu that is open blocks the page behind it: close it first (its close button, or browser_key Escape) before acting on anything else. \
         After each action call browser_elements or browser_dom again to see the result, and check browser_console for errors. \
         When the message names or attaches a page element (a selector, its text, its markup, where it sits), the change belongs to the code that renders that element: search for the words it shows to find the template, and edit exactly there. Answer in the user's language.{}",
        project_brief(cwd)
    );
    if !skills.is_empty() {
        prompt.push_str("\n\nSkills you can load with the Skill tool when the task matches (name: what it is for):");
        for skill in skills.iter().take(60) {
            prompt.push_str(&format!(
                "\n- {}: {}",
                skill.name,
                skill.description.chars().take(110).collect::<String>()
            ));
        }
    }
    prompt
}

/// The skills and commands the user has for Claude, Codex and Antigravity (each name once), for the Skill tool.
fn skills_for(cwd: Option<&Path>) -> Vec<ProviderCommand> {
    let Ok(home) = crate::home_folder() else {
        return Vec::new();
    };
    let project = cwd.filter(|dir| dir.is_dir());
    let mut seen = std::collections::HashSet::new();
    ["claude", "codex", "antigravity"]
        .iter()
        .flat_map(|agent| slash_commands::list(agent, &home, project))
        .filter(|item| seen.insert(item.name.clone()))
        .collect()
}

/// A tool name an OpenAI function may carry: letters, digits, `_` and `-`, at most 64 characters.
fn tool_name(server: &str, tool: &str) -> String {
    let clean = |text: &str| {
        text.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
    };
    format!("mcp__{}__{}", clean(server), clean(tool))
        .chars()
        .take(64)
        .collect()
}

/// Starts the user's MCP servers at the same time; one that fails or is slow is left out.
async fn start_servers(cwd: Option<&Path>) -> Vec<Server> {
    let project = cwd.map(Path::to_path_buf);
    let raws = tauri::async_runtime::spawn_blocking(move || {
        crate::mcp_manage::servers_for(project.as_deref())
    })
    .await
    .unwrap_or_default();
    let handles: Vec<_> = raws
        .into_iter()
        .map(|raw| {
            tokio::spawn(async move {
                tokio::time::timeout(Duration::from_secs(30), Server::start(&raw)).await
            })
        })
        .collect();
    let mut servers = Vec::new();
    for handle in handles {
        if let Ok(Ok(Ok(server))) = handle.await {
            servers.push(server);
        }
    }
    servers
}

/// Runs one turn: the conversation goes to the model, its tool calls are run and answered, until it replies in
/// plain text. Ends with `Done`, `PermissionDenied` then `Done`, or `Error`.
pub async fn run_turn(
    app: &AppHandle,
    mut turn: Turn,
    history: Vec<ChatMessage>,
    mut emit: impl FnMut(AgentEvent),
) {
    if !(turn.base_url.starts_with("http://") || turn.base_url.starts_with("https://")) {
        return emit(AgentEvent::Error {
            message: "the provider address must start with http:// or https://".into(),
        });
    }
    if turn.kind == "anthropic" && !protocol::messages_provider(&turn) {
        return emit(AgentEvent::Error {
            message: "Anthropic-compatible providers cannot chat yet".into(),
        });
    }
    if turn.restricted {
        if let Some(problem) = restricted_leak(&history) {
            return emit(AgentEvent::Error {
                message: format!("Not sent: {problem}. This free-tier provider may keep and train on what it gets: remove it, or ask Claude, Codex or Antigravity instead."),
            });
        }
    }
    let client = match reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return emit(AgentEvent::Error {
                message: error.to_string(),
            })
        }
    };
    let chat: String = turn
        .chat
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || "-_:".contains(*c))
        .collect();
    let mut tools = builtin_tools();
    if turn.restricted {
        tools.retain(|tool| {
            RESTRICTED_TOOLS.contains(&tool["function"]["name"].as_str().unwrap_or_default())
        });
    }
    if let Some(folder) = &turn.cwd {
        crate::index::set_root(&chat, &folder.to_string_lossy());
    }
    if !turn.restricted {
        tools.extend(browser_tools(app, &chat).await);
    }
    let skills = if turn.restricted {
        Vec::new()
    } else {
        skills_for(turn.cwd.as_deref())
    };
    // The user's own MCP servers: each tool is offered as `mcp__<server>__<tool>`, as Claude names them. A restricted
    // provider gets none: they run on this machine, with the user's accounts.
    let mut servers = if turn.restricted {
        Vec::new()
    } else {
        start_servers(turn.cwd.as_deref()).await
    };
    let mut external: Vec<(String, usize, String)> = Vec::new();
    for (index, server) in servers.iter().enumerate() {
        for tool in &server.tools {
            let Some(name) = tool["name"].as_str() else {
                continue;
            };
            let exposed = tool_name(&server.name, name);
            let properties = if tool["inputSchema"]["properties"].is_object() {
                tool["inputSchema"]["properties"].clone()
            } else {
                json!({})
            };
            let required: Vec<&str> = tool["inputSchema"]["required"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            tools.push(function(
                &exposed,
                tool["description"].as_str().unwrap_or_default(),
                properties,
                &required,
            ));
            external.push((exposed, index, name.to_owned()));
        }
    }

    let mut messages: Vec<Value> = vec![
        json!({ "role": "system", "content": if turn.restricted { RESTRICTED_PROMPT.to_string() } else { system_prompt(turn.cwd.as_deref(), &skills) } }),
    ];
    // Workers AI's OpenAI-compatible route checks each message against its own schema: only text content, never null.
    let text_only = turn.base_url.contains("api.cloudflare.com");
    messages.extend(
        history
            .iter()
            .map(|message| wire_message(message, text_only)),
    );
    // A message the user writes while this turn runs goes into the conversation at the next step, as a CLI that reads
    // its input at any time would take it: the work goes on, and the answer covers it.
    let (_inbox_slot, mut inbox) = InjectSlot::open(&turn.chat);

    let mut answer = String::new();
    let mut retries = 0;
    // Guard rails that do not depend on how well the model follows the prompt: it must plan before it changes files, change
    // only planned files, read a file before editing it, and check its work before it answers.
    let mut planned: Option<std::collections::HashSet<String>> = None;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut edited = false;
    let mut checked = true;
    let mut nudged = false;
    let mut finish_nudged = false;
    let mut last_call_signature = String::new();
    let mut repeated_calls = 0;
    // Searches the engine refused with 429 this turn: after a few the turn stops instead of asking again.
    let mut rate_limited_searches = 0;
    // A model that spends its whole answer on thinking (some local ones do, with big prompts): it is asked again, then asked
    // again with thinking off, before the turn is called a failure.
    let mut silent = 0;
    for step_number in 0..MAX_STEPS {
        while let Ok(text) = inbox.try_recv() {
            messages.push(json!({ "role": "user", "content": text }));
        }
        let step = match model_call(&client, &turn, &messages, &tools, &mut emit).await {
            Ok(step) => step,
            Err(message) => return emit(AgentEvent::Error { message }),
        };
        emit(AgentEvent::Usage {
            input_tokens: step.input_tokens,
            output_tokens: step.output_tokens,
            cache_read_tokens: step.cached_tokens,
            cache_write_tokens: 0,
            new_call: true,
        });
        answer.push_str(&step.text);
        let calls: Vec<&Call> = step
            .calls
            .iter()
            .filter(|call| !call.name.is_empty())
            .collect();
        if calls.is_empty() {
            // Some local models write the tool call as text instead of using the tool interface: ask once more.
            if (step.text.contains("<tool_call>") || step.text.contains("<function="))
                && retries < 2
            {
                retries += 1;
                let mut assistant = json!({ "role": "assistant", "content": step.text });
                if !step.reasoning.is_empty() {
                    assistant["reasoning_content"] = step.reasoning.clone().into();
                }
                messages.push(assistant);
                messages.push(json!({ "role": "user", "content": "That tool call was written as text, so it did not run. Call the tool through the tool-calling interface." }));
                continue;
            }
            if edited && !checked && !nudged && !step.text.trim().is_empty() {
                // It changed files and is about to answer without having run anything since: ask once for the check.
                nudged = true;
                messages.push(json!({ "role": "assistant", "content": step.text }));
                messages.push(json!({ "role": "user", "content": "You changed files but have not checked the result. Before you answer: run the project's build, lint, type check or tests with Bash (see its package.json scripts or Cargo.toml), or look at the Preview, fix what fails, then give your final answer." }));
                continue;
            }
            if step.finish == "length" {
                return emit(AgentEvent::Error { message: "the model hit its output limit before it finished: raise the limit or the context in the server, or ask for a smaller step".into() });
            }
            if answer.trim().is_empty() {
                if silent < 2 {
                    silent += 1;
                    if silent == 2 {
                        turn.effort = Some("off".into());
                    }
                    messages.push(json!({ "role": "user", "content": "You only thought and wrote no answer. Do not think again: call a tool now, or write your answer." }));
                    continue;
                }
                return emit(AgentEvent::Error { message: "the model gave no answer (it only thought), even with thinking off. Pick another model, or try a smaller step".into() });
            }
            // A message that came in while the model was writing its last answer is not left unread: the work goes on.
            let mut late = false;
            while let Ok(text) = inbox.try_recv() {
                if !late {
                    let mut assistant = json!({ "role": "assistant", "content": step.text });
                    if !step.reasoning.is_empty() {
                        assistant["reasoning_content"] = step.reasoning.clone().into();
                    }
                    messages.push(assistant);
                    late = true;
                }
                messages.push(json!({ "role": "user", "content": text }));
            }
            if late {
                continue;
            }
            return emit(AgentEvent::Done {
                session_id: None,
                text: answer,
                cost_usd: None,
            });
        }

        let signature = calls
            .iter()
            .map(|call| format!("{}:{}", call.name, call.arguments))
            .collect::<Vec<_>>()
            .join("|");
        if signature == last_call_signature {
            repeated_calls += 1;
        } else {
            last_call_signature = signature;
            repeated_calls = 1;
        }

        let ids: Vec<String> = calls
            .iter()
            .enumerate()
            .map(|(i, call)| {
                if call.id.is_empty() {
                    format!("call_{}_{i}", messages.len())
                } else {
                    call.id.clone()
                }
            })
            .collect();
        let mut assistant = json!({
            "role": "assistant",
            "content": step.text.clone(),
            "tool_calls": calls.iter().zip(&ids).map(|(call, id)| json!({ "id": id, "type": "function", "function": { "name": call.name, "arguments": if call.arguments.is_empty() { "{}" } else { call.arguments.as_str() } } })).collect::<Vec<_>>(),
        });
        if !step.reasoning.is_empty() {
            assistant["reasoning_content"] = step.reasoning.clone().into();
        }
        messages.push(assistant);

        // Every call of the step is shown as running at once. Those that only wait (on a sub-agent, on the web) start
        // together, so the slowest sets the time and not the sum; the rest run in order after them. The tasks live in
        // a set: when the turn is stopped it is dropped and they end with it, a sub-agent nobody waits for included.
        let mut side_by_side = tokio::task::JoinSet::new();
        let mut prepared: Vec<Value> = Vec::with_capacity(calls.len());
        for (call, id) in calls.iter().zip(&ids) {
            let args: Value = serde_json::from_str(if call.arguments.is_empty() {
                "{}"
            } else {
                &call.arguments
            })
            .unwrap_or(Value::Null);
            let user_tool = external
                .iter()
                .find(|(exposed, _, _)| *exposed == call.name);
            let builtin = matches!(
                call.name.as_str(),
                "Bash"
                    | "Read"
                    | "Write"
                    | "Edit"
                    | "Glob"
                    | "Grep"
                    | "WebFetch"
                    | "WebSearch"
                    | "Skill"
                    | "LS"
                    | "MkDir"
                    | "Move"
                    | "Copy"
                    | "Delete"
                    | "Plan"
                    | "PackageInfo"
            );
            let shown = if builtin || user_tool.is_some() {
                call.name.clone()
            } else {
                format!("{MCP_PREFIX}{}", call.name)
            };
            emit(AgentEvent::ToolCall {
                id: id.clone(),
                name: shown.clone(),
                input: if call.name == "delegate_task" {
                    args.to_string()
                } else {
                    summarize_tool_input(&call.name, &args)
                },
                change: change_for(&call.name, &args, turn.cwd.as_deref()),
                background: (crate::delegate::is_tool(&call.name) && args["wait"] == false)
                    || ag_adapters::agent::delegate_wait(&shown, &args),
            });
            let waits_only = user_tool.is_none()
                && args.is_object()
                && (crate::delegate::is_tool(&call.name)
                    || matches!(call.name.as_str(), "WebFetch" | "WebSearch" | "PackageInfo"));
            if waits_only && matches!(decide(&turn, &call.name, &args, false), Decision::Allow) {
                let (app, chat, name, args, id, restricted) = (
                    app.clone(),
                    chat.clone(),
                    call.name.clone(),
                    args.clone(),
                    id.clone(),
                    turn.restricted,
                );
                side_by_side.spawn(async move {
                    let outcome = match name.as_str() {
                        "WebFetch" => web_fetch(&args, restricted).await,
                        "WebSearch" => web_search(&args).await,
                        "PackageInfo" => package_info(&args).await,
                        _ => browser_call(&app, &chat, &name, &args).await,
                    };
                    (id, outcome)
                });
            }
            prepared.push(args);
        }
        let mut finished: std::collections::HashMap<String, Outcome> =
            std::collections::HashMap::new();
        while let Some(joined) = side_by_side.join_next().await {
            if let Ok((id, outcome)) = joined {
                finished.insert(id, outcome);
            }
        }

        let mut denied = Vec::new();
        for ((call, id), args) in calls.iter().zip(&ids).zip(prepared) {
            let user_tool = external
                .iter()
                .find(|(exposed, _, _)| *exposed == call.name);
            let outcome = if let Some(done) = finished.remove(id) {
                done
            } else if !args.is_object() {
                failed("the arguments were not valid JSON")
            } else {
                match decide(&turn, &call.name, &args, user_tool.is_some()) {
                    Decision::Deny(reason) => failed(reason),
                    Decision::Ask => {
                        denied.push(PermissionRequest { id: id.clone(), tool: call.name.clone(), summary: summarize_tool_input(&call.name, &args), allow: allow_entry(&call.name, &args) });
                        failed("this needs the user's approval; it was not run")
                    }
                    Decision::Careful(reason) => {
                        denied.push(PermissionRequest { id: id.clone(), tool: call.name.clone(), summary: format!("⚠ {reason} {}", summarize_tool_input(&call.name, &args)), allow: allow_entry(&call.name, &args) });
                        failed("this is risky and needs the user's approval each time; it was not run")
                    }
                    Decision::Allow => match call.name.as_str() {
                        "Bash" => {
                            checked = true;
                            run_bash(args["command"].as_str().unwrap_or_default(), turn.cwd.as_deref()).await
                        }
                        "Read" => {
                            let outcome = read_file(&args, turn.cwd.as_deref());
                            if !outcome.is_error {
                                seen.insert(path_key(turn.cwd.as_deref(), args["file_path"].as_str().unwrap_or_default()));
                            }
                            outcome
                        }
                        "Plan" => {
                            let (outcome, files) = plan_tool(&args, turn.cwd.as_deref());
                            if let Some(files) = files {
                                planned = Some(files);
                            }
                            outcome
                        }
                        "PackageInfo" => package_info(&args).await,
                        "Write" | "Edit" => {
                            let path = args["file_path"].as_str().unwrap_or_default();
                            let key = path_key(turn.cwd.as_deref(), path);
                            let exists = resolve(turn.cwd.as_deref(), path).exists();
                            let outcome = match &planned {
                                None => failed("Call Plan first: say which files you will change (found with code_search, Grep or Glob), the steps, and how you will check. Then do the edit."),
                                Some(files) if !files.contains(&key) => failed(format!("{path} is not in your plan. Change a file that is, or call Plan again with the full list if this one really must change too.")),
                                Some(_) if exists && !seen.contains(&key) => failed(format!("Read {path} before you change it, so what you replace is copied exactly from the file.")),
                                Some(_) if call.name == "Write" => write_file(&args, turn.cwd.as_deref()),
                                Some(_) => edit_file(&args, turn.cwd.as_deref()),
                            };
                            if !outcome.is_error {
                                seen.insert(key);
                                edited = true;
                                checked = false;
                            }
                            outcome
                        }
                        "Delete" | "Move" if planned.is_none() => failed("Call Plan first: say which files you will change and how you will check, then do this."),
                        "Glob" | "Grep" => {
                            let (name, cwd, args) = (call.name.clone(), turn.cwd.clone(), args.clone());
                            tokio::task::spawn_blocking(move || if name == "Glob" { glob_files(&args, cwd.as_deref()) } else { grep_files(&args, cwd.as_deref()) })
                                .await
                                .unwrap_or_else(|_| failed("the search failed"))
                        }
                        "WebFetch" => web_fetch(&args, turn.restricted).await,
                        "WebSearch" => web_search(&args).await,
                        "LS" => list_dir(&args, turn.cwd.as_deref()),
                        "MkDir" => make_dir(&args, turn.cwd.as_deref()),
                        "Move" => transfer(&args, turn.cwd.as_deref(), true),
                        "Copy" => transfer(&args, turn.cwd.as_deref(), false),
                        "Delete" => delete_path(&args, turn.cwd.as_deref()),
                        "Skill" => {
                            let wanted = args["name"].as_str().unwrap_or_default().trim_start_matches('/');
                            match (skills.iter().find(|item| item.name == wanted), crate::home_folder()) {
                                (Some(skill), Ok(home)) => match slash_commands::read(Path::new(&skill.path), &home, turn.cwd.as_deref()) {
                                    Ok(text) => ok(trim_output(&text, OUTPUT_MAX * 2, START_HEAVY)),
                                    Err(error) => failed(error),
                                },
                                _ => failed(format!("no skill named {wanted}")),
                            }
                        }
                        name => match user_tool {
                            Some((_, index, tool)) => {
                                let reply = servers[*index].call(tool, &args).await;
                                Outcome { text: trim_output(&reply.text, OUTPUT_MAX, BALANCED), is_error: reply.is_error, image: reply.image }
                            }
                            None => {
                                if name.starts_with("browser_") {
                                    checked = true;
                                }
                                // A delegated task has its own timeout (it may take many minutes); the browser tools do not.
                                if crate::delegate::is_tool(name) {
                                    browser_call(app, &chat, name, &args).await
                                } else {
                                    tokio::time::timeout(TOOL_LIMIT, browser_call(app, &chat, name, &args)).await.unwrap_or_else(|_| failed("the browser did not answer in time. Try browser_open again, then browser_elements."))
                                }
                            }
                        },
                    },
                }
            };
            if call.name == "WebSearch" && outcome.is_error && outcome.text.contains("429") {
                rate_limited_searches += 1;
            }
            emit(AgentEvent::ToolResult {
                id: id.clone(),
                output: outcome.text.clone(),
                is_error: outcome.is_error,
                image: outcome.image.clone(),
                change: None,
            });
            messages.push(json!({ "role": "tool", "tool_call_id": id, "content": outcome.text }));
        }
        if rate_limited_searches >= SEARCH_LIMIT_HITS {
            return emit(AgentEvent::Error { message: "the web search was refused (429 Too Many Requests) again and again; the turn stopped instead of retrying. Try again later or with another search.".into() });
        }
        if !denied.is_empty() {
            emit(AgentEvent::PermissionDenied { requests: denied });
            return emit(AgentEvent::Done {
                session_id: None,
                text: answer,
                cost_usd: None,
            });
        }

        if repeated_calls >= REPEATED_CALL_LIMIT {
            return emit(AgentEvent::Error { message: "the model repeated the same tool call without making progress; shorten the task or turn thinking off for this local model".into() });
        }
        if step_number + 1 >= FINAL_NUDGE_AT && !finish_nudged {
            finish_nudged = true;
            messages.push(json!({ "role": "user", "content": "Stop using tools now. Finish this step with a concise final answer and a `## Handoff` section. Do not call another tool unless it is strictly required to report the result." }));
        }
    }
    emit(AgentEvent::Error {
        message: format!("stopped after {MAX_STEPS} steps without a final answer"),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_character_split_across_chunks_stays_whole() {
        // "ệ" is three UTF-8 bytes: cut it between two network chunks.
        let text = "data: {\"t\":\"Việt\"}\n".as_bytes();
        let cut = text
            .iter()
            .position(|byte| *byte == 0xE1)
            .map(|at| at + 1)
            .expect("contains ệ");
        let mut pending = Vec::new();
        pending.extend_from_slice(&text[..cut]);
        assert_eq!(take_line(&mut pending), None);
        pending.extend_from_slice(&text[cut..]);
        assert_eq!(
            take_line(&mut pending).as_deref(),
            Some("data: {\"t\":\"Việt\"}\n")
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn sse_lines_survive_every_chunk_boundary_and_keep_an_unterminated_tail() {
        let first = "data: {\"t\":\"Tiếng Việt 😀\"}\r\n";
        let second = "data: [DONE]\n";
        let text = format!("{first}{second}data: unfinished");
        for cut in 0..=text.len() {
            let mut pending = Vec::new();
            let mut lines = Vec::new();
            for chunk in [&text.as_bytes()[..cut], &text.as_bytes()[cut..]] {
                pending.extend_from_slice(chunk);
                while let Some(line) = take_line(&mut pending) {
                    lines.push(line);
                }
            }
            assert_eq!(lines, [first, second], "chunk boundary {cut}");
            assert_eq!(pending, b"data: unfinished");
            assert_eq!(take_line(&mut pending), None);
        }
    }

    fn turn(mode: Option<&str>, allowed: &[&str]) -> Turn {
        Turn {
            kind: "openai".into(),
            base_url: String::new(),
            api_key: None,
            key_env: None,
            model: String::new(),
            cwd: None,
            permission_mode: mode.map(str::to_owned),
            allowed: allowed.iter().map(|s| s.to_string()).collect(),
            chat: String::new(),
            effort: None,
            restricted: false,
            workbuddy: false,
        }
    }

    #[test]
    fn a_restricted_provider_may_only_use_the_public_web() {
        let mut restricted = turn(Some("bypassPermissions"), &[]);
        restricted.restricted = true;
        let call = |name: &str, external: bool| {
            matches!(
                decide(&restricted, name, &json!({}), external),
                Decision::Allow
            )
        };
        assert!(call("WebSearch", false) && call("WebFetch", false) && call("PackageInfo", false));
        for name in [
            "Read",
            "Grep",
            "Glob",
            "LS",
            "Bash",
            "Write",
            "Edit",
            "Delete",
            "browser_screenshot",
            "code_search",
            "delegate_task",
        ] {
            assert!(!call(name, false), "{name} must be denied");
        }
        assert!(!call("mcp__github__create_issue", true));
        // Every tool of AgentDeck's own MCP server (browser, code index, checks, delegation, the computer) reaches this
        // machine or another agent that reads it: none is a way around the rule, today or when one is added.
        for tool in crate::mcp::tool_list().as_array().into_iter().flatten() {
            let name = tool["name"].as_str().unwrap_or_default();
            assert!(!call(name, false), "{name} must be denied");
            assert!(
                !call(&format!("{MCP_PREFIX}{name}"), false),
                "{name} must be denied"
            );
        }
        // Nor can it reach that server, or anything else on this machine, through its web tools.
        let fetch = |url: &str| {
            matches!(
                decide(&restricted, "WebFetch", &json!({ "url": url }), false),
                Decision::Allow
            )
        };
        assert!(!fetch("http://127.0.0.1:47821/2b283ce45cca/mcp"));
        assert!(!fetch("http://localhost:1420/"));
        assert!(fetch("https://example.com/"));
        // The same tools are fine for a trusted provider.
        assert!(matches!(
            decide(
                &turn(Some("bypassPermissions"), &[]),
                "Bash",
                &json!({}),
                false
            ),
            Decision::Allow | Decision::Ask | Decision::Careful(_)
        ));
    }

    #[test]
    fn edit_matches_text_across_line_ends_and_says_where_a_miss_is_close() {
        let dir = std::env::temp_dir().join(format!("agentdeck-edit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("a.ts");
        std::fs::write(&file, "one\r\ntwo\r\nthree\r\n").expect("write");
        let miss = edit_file(
            &json!({ "file_path": file.to_string_lossy(), "old_string": "   three  ", "new_string": "x" }),
            None,
        );
        assert!(
            miss.is_error && miss.text.contains("closest place"),
            "{}",
            miss.text
        );
        let args = json!({ "file_path": file.to_string_lossy(), "old_string": "two\nthree", "new_string": "2\n3" });
        let done = edit_file(&args, None);
        assert!(!done.is_error, "{}", done.text);
        assert_eq!(
            std::fs::read_to_string(&file).expect("read"),
            "one\r\n2\r\n3\r\n"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_plan_names_its_files() {
        let (outcome, files) = plan_tool(
            &json!({ "goal": "g", "files": ["src/a.ts"], "steps": ["do it"] }),
            None,
        );
        assert!(!outcome.is_error && files.is_some_and(|set| set.len() == 1));
        let (outcome, files) = plan_tool(&json!({ "goal": "g", "files": [], "steps": [] }), None);
        assert!(outcome.is_error && files.is_none());
    }

    #[test]
    fn chat_route_follows_the_kind() {
        assert_eq!(
            chat_url("openai", "http://h:1234/v1/"),
            "http://h:1234/v1/chat/completions"
        );
        assert_eq!(
            chat_url("ollama", "http://h:11434"),
            "http://h:11434/v1/chat/completions"
        );
    }

    #[test]
    fn responses_use_provider_specific_open_models_only() {
        let base = "https://api.commandcode.ai/provider/v1";
        let make = |model: &str| Turn {
            kind: "openai".into(),
            base_url: base.into(),
            api_key: None,
            key_env: None,
            model: model.into(),
            cwd: None,
            permission_mode: None,
            allowed: Vec::new(),
            chat: String::new(),
            effort: None,
            restricted: false,
            workbuddy: false,
        };
        assert!(responses_provider(&make("deepseek/deepseek-v4-flash")));
        let mut muse = make("muse-spark-1.3");
        muse.base_url = "https://api.meta.ai/v1".into();
        assert!(responses_provider(&muse));
        assert!(!responses_provider(&make("anthropic/claude-sonnet-4-6")));
        let mut other = make("gpt-4o");
        other.base_url = "https://api.openai.com/v1".into();
        assert!(!responses_provider(&other));
    }

    #[test]
    fn responses_input_preserves_tool_round_trips() {
        let messages = vec![
            json!({ "role": "system", "content": "be concise" }),
            json!({ "role": "user", "content": [{ "type": "text", "text": "look" }, { "type": "image_url", "image_url": { "url": "data:image/png;base64,AAAA" } }] }),
            json!({ "role": "assistant", "content": "", "tool_calls": [{ "id": "call_1", "type": "function", "function": { "name": "Bash", "arguments": "{\"command\":\"pwd\"}" } }] }),
            json!({ "role": "tool", "tool_call_id": "call_1", "content": "C:/project" }),
        ];
        let input = responses_input(&messages);
        assert_eq!(input[1]["content"][0]["type"], "input_text");
        assert_eq!(input[1]["content"][1]["type"], "input_image");
        assert_eq!(input[2]["type"], "function_call");
        assert_eq!(input[2]["call_id"], "call_1");
        assert_eq!(input[3]["type"], "function_call_output");
        assert_eq!(input[3]["output"], "C:/project");
    }

    #[test]
    fn messages_provider_accepts_claude_on_command_code_and_muse() {
        let mut turn = turn(None, &[]);
        turn.base_url = "https://api.commandcode.ai/provider/v1".into();
        turn.model = "anthropic/claude-sonnet-4-6".into();
        assert!(messages_provider(&turn));
        turn.base_url = "https://api.meta.ai/v1".into();
        assert!(messages_provider(&turn));
        turn.model = "muse-spark-1.3".into();
        assert!(!messages_provider(&turn));
    }

    #[test]
    fn messages_stream_maps_text_thinking_and_tool_input() {
        let mut step = Step::default();
        let mut text = String::new();
        let mut thinking = String::new();
        let mut emit = |event| match event {
            AgentEvent::Delta { text: part } => text.push_str(&part),
            AgentEvent::Thinking { text: part } => thinking.push_str(&part),
            _ => {}
        };
        read_messages_chunk(
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tool_1","name":"Bash","input":{}}}"#,
            &mut step,
            &mut emit,
        );
        read_messages_chunk(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"plan"}}"#,
            &mut step,
            &mut emit,
        );
        read_messages_chunk(
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"command\":\"pwd\"}"}}"#,
            &mut step,
            &mut emit,
        );
        read_messages_chunk(
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"done"}}"#,
            &mut step,
            &mut emit,
        );
        read_messages_chunk(
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":7}}"#,
            &mut step,
            &mut emit,
        );
        assert_eq!(thinking, "plan");
        assert_eq!(text, "done");
        assert_eq!(step.finish, "tool_calls");
        assert_eq!(step.output_tokens, 7);
        assert_eq!(step.calls[0].name, "Bash");
        assert_eq!(step.calls[0].arguments, r#"{"command":"pwd"}"#);
    }

    #[test]
    fn responses_stream_maps_text_reasoning_and_function_calls() {
        let mut step = Step::default();
        let mut text = String::new();
        let mut thinking = String::new();
        let mut emit = |event| match event {
            AgentEvent::Delta { text: part } => text.push_str(&part),
            AgentEvent::Thinking { text: part } => thinking.push_str(&part),
            _ => {}
        };
        read_responses_chunk(
            r#"{"type":"response.output_text.delta","delta":"hello"}"#,
            &mut step,
            &mut emit,
        );
        read_responses_chunk(
            r#"{"type":"response.reasoning_summary_text.delta","delta":"think"}"#,
            &mut step,
            &mut emit,
        );
        read_responses_chunk(
            r#"{"type":"response.output_item.added","item":{"type":"function_call","id":"item_1","call_id":"call_1","name":"Bash","arguments":""}}"#,
            &mut step,
            &mut emit,
        );
        read_responses_chunk(
            r#"{"type":"response.function_call_arguments.delta","item_id":"item_1","delta":"{\"command\":\"pwd\"}"}"#,
            &mut step,
            &mut emit,
        );
        read_responses_chunk(
            r#"{"type":"response.completed","response":{"usage":{"input_tokens":10,"output_tokens":3}}}"#,
            &mut step,
            &mut emit,
        );
        assert_eq!(text, "hello");
        assert_eq!(thinking, "think");
        assert_eq!(step.calls[0].id, "call_1");
        assert_eq!(step.calls[0].arguments, "{\"command\":\"pwd\"}");
        assert_eq!(step.finish, "tool_calls");
        assert_eq!((step.input_tokens, step.output_tokens), (10, 3));
    }

    #[test]
    fn cached_prompt_tokens_are_counted_apart() {
        let mut step = Step::default();
        let mut emit = |_| {};
        read_chunk(
            r#"{"choices":[],"usage":{"prompt_tokens":1000,"completion_tokens":9,"prompt_tokens_details":{"cached_tokens":800}}}"#,
            &mut step,
            &mut emit,
        );
        assert_eq!(
            (step.input_tokens, step.cached_tokens, step.output_tokens),
            (200, 800, 9)
        );
        read_chunk(
            r#"{"choices":[],"usage":{"prompt_tokens":50,"completion_tokens":1,"prompt_cache_hit_tokens":30}}"#,
            &mut step,
            &mut emit,
        );
        assert_eq!((step.input_tokens, step.cached_tokens), (20, 30));
    }

    #[test]
    fn a_text_only_api_gets_string_content_and_pictures_named() {
        let message = ChatMessage {
            role: "user".into(),
            content: "look".into(),
            images: vec!["data:image/png;base64,AAAA".into()],
            files: Vec::new(),
        };
        let text = wire_message(&message, true);
        assert_eq!(
            text["content"],
            "look\n\n[1 image(s) attached: this model reads text only]"
        );
        let parts = wire_message(&message, false);
        assert_eq!(parts["content"][1]["type"], "image_url");
    }

    #[test]
    fn an_sse_line_without_its_newline_still_counts() {
        let mut step = Step::default();
        let mut shown = String::new();
        let mut emit = |event: AgentEvent| {
            if let AgentEvent::Delta { text } = event {
                shown.push_str(&text);
            }
        };
        let last = r#"data: {"choices":[{"delta":{"content":"hết."}}]}"#;
        assert!(!handle_sse_line(last, &mut step, &mut emit));
        assert_eq!(step.text, "hết.");
        assert_eq!(shown, "hết.");
    }

    #[test]
    fn chunks_give_text_reasoning_tool_calls_and_usage() {
        let mut step = Step::default();
        let mut events = Vec::new();
        let mut emit = |event| events.push(event);
        read_chunk(
            r#"{"choices":[{"delta":{"reasoning_content":"hm","content":"hi"}}]}"#,
            &mut step,
            &mut emit,
        );
        assert_eq!(step.reasoning, "hm");
        read_chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"Bash","arguments":"{\"comm"}}]}}]}"#,
            &mut step,
            &mut emit,
        );
        read_chunk(
            r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"and\":\"ls\"}"}}]}}]}"#,
            &mut step,
            &mut emit,
        );
        read_chunk(
            r#"{"choices":[],"usage":{"prompt_tokens":3,"completion_tokens":5}}"#,
            &mut step,
            &mut emit,
        );
        read_chunk("not json", &mut step, &mut emit);
        assert_eq!(
            events,
            [
                AgentEvent::Thinking { text: "hm".into() },
                AgentEvent::Delta { text: "hi".into() }
            ]
        );
        assert_eq!(
            (step.text.as_str(), step.input_tokens, step.output_tokens),
            ("hi", 3, 5)
        );
        assert_eq!(
            (
                step.calls[0].id.as_str(),
                step.calls[0].name.as_str(),
                step.calls[0].arguments.as_str()
            ),
            ("c1", "Bash", r#"{"command":"ls"}"#)
        );
    }

    #[test]
    fn the_guard_refuses_what_cannot_be_undone_in_every_mode_and_after_an_approval() {
        let wipe = json!({ "command": "rm -rf /" });
        let drop = json!({ "command": "psql -c \"DROP DATABASE shop\"" });
        let ordinary = json!({ "command": "cargo build" });
        for mode in [
            None,
            Some("acceptEdits"),
            Some("auto"),
            Some("bypassPermissions"),
        ] {
            assert!(
                matches!(
                    decide(&turn(mode, &[]), "Bash", &wipe, false),
                    Decision::Deny(_)
                ),
                "{mode:?}"
            );
            assert!(
                matches!(
                    decide(&turn(mode, &[]), "Bash", &drop, false),
                    Decision::Deny(_)
                ),
                "{mode:?}"
            );
        }
        // Not even a command the user allowed before.
        assert!(matches!(
            decide(&turn(None, &["Bash(rm -rf /)"]), "Bash", &wipe, false),
            Decision::Deny(_)
        ));
        // An ordinary command is still the mode's business.
        assert!(matches!(
            decide(
                &turn(Some("bypassPermissions"), &[]),
                "Bash",
                &ordinary,
                false
            ),
            Decision::Allow
        ));
    }

    #[test]
    fn a_risky_command_is_asked_about_each_time_in_every_mode_but_plan() {
        let risky = json!({ "command": "git push --force origin main" });
        for mode in [
            None,
            Some("acceptEdits"),
            Some("auto"),
            Some("bypassPermissions"),
        ] {
            assert!(
                matches!(
                    decide(&turn(mode, &[]), "Bash", &risky, false),
                    Decision::Careful(_)
                ),
                "{mode:?}"
            );
        }
        // An earlier "always allow" does not count for it.
        assert!(matches!(
            decide(
                &turn(Some("auto"), &["Bash(git push --force origin main)"]),
                "Bash",
                &risky,
                false
            ),
            Decision::Careful(_)
        ));
        // Plan mode still refuses every command.
        assert!(matches!(
            decide(&turn(Some("plan"), &[]), "Bash", &risky, false),
            Decision::Deny(_)
        ));
    }

    #[test]
    fn long_output_keeps_its_start_and_end() {
        let long = format!("first line\n{}last line: error", "noise\n".repeat(10_000));
        let command = trim_output(&long, OUTPUT_MAX, END_HEAVY);
        assert!(command.chars().count() <= OUTPUT_MAX);
        assert!(command.starts_with("first line") && command.ends_with("last line: error"));
        assert!(command.contains("characters pruned"));
        // A command keeps more of its end than a file does.
        let file = trim_output(&long, OUTPUT_MAX, START_HEAVY);
        let noise = |text: &str| text.matches("noise").count();
        assert!(noise(&command) > 0 && noise(&file) > 0);
    }

    #[test]
    fn permissions_follow_the_mode() {
        let write = json!({ "file_path": "a.txt", "content": "x" });
        let bash = json!({ "command": "git status" });
        assert!(matches!(
            decide(&turn(None, &[]), "Read", &Value::Null, false),
            Decision::Allow
        ));
        assert!(matches!(
            decide(&turn(None, &[]), "Write", &write, false),
            Decision::Ask
        ));
        assert!(matches!(
            decide(&turn(Some("acceptEdits"), &[]), "Write", &write, false),
            Decision::Allow
        ));
        assert!(matches!(
            decide(&turn(Some("acceptEdits"), &[]), "Bash", &bash, false),
            Decision::Ask
        ));
        assert!(matches!(
            decide(&turn(Some("auto"), &[]), "Bash", &bash, false),
            Decision::Allow
        ));
        assert!(matches!(
            decide(&turn(Some("plan"), &[]), "Bash", &bash, false),
            Decision::Deny(_)
        ));
        assert!(matches!(
            decide(&turn(None, &["Bash(git status)"]), "Bash", &bash, false),
            Decision::Allow
        ));
        assert!(matches!(
            decide(&turn(None, &["Write"]), "Write", &write, false),
            Decision::Allow
        ));
    }

    #[test]
    fn files_can_be_made_found_moved_and_deleted_inside_the_project() {
        let dir = std::env::temp_dir().join(format!("agentdeck-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cwd = Some(dir.as_path());
        assert!(!make_dir(&json!({ "path": "src/lib" }), cwd).is_error);
        assert!(
            !write_file(
                &json!({ "file_path": "src/lib/a.ts", "content": "let needle = 1;\n" }),
                cwd
            )
            .is_error
        );
        assert_eq!(
            glob_files(&json!({ "pattern": "*.ts" }), cwd).text,
            "src/lib/a.ts"
        );
        assert!(grep_files(&json!({ "pattern": "needle" }), cwd)
            .text
            .starts_with("src/lib/a.ts:1:"));
        assert!(list_dir(&json!({ "path": "src" }), cwd)
            .text
            .contains("lib/"));
        assert!(
            !transfer(
                &json!({ "from": "src/lib/a.ts", "to": "src/b.ts" }),
                cwd,
                true
            )
            .is_error
        );
        assert!(transfer(&json!({ "from": "src/b.ts", "to": "src/lib" }), cwd, false).is_error);
        assert!(delete_path(&json!({ "path": ".." }), cwd).is_error);
        assert!(!delete_path(&json!({ "path": "src" }), cwd).is_error);
        assert!(!dir.join("src").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edit_needs_one_exact_match() {
        let dir = std::env::temp_dir().join(format!("agentdeck-custom-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.txt"), "a b a").unwrap();
        let edit = |old: &str| {
            edit_file(
                &json!({ "file_path": "f.txt", "old_string": old, "new_string": "z" }),
                Some(&dir),
            )
        };
        assert!(edit("a").is_error);
        assert!(edit("q").is_error);
        assert!(!edit("b").is_error);
        assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "a z a");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_brave_results_page_gives_title_address_and_snippet() {
        let page = r#"<div class="snippet svelte-1" data-pos="1" data-type="web"><a href="https://www.cloudflare.com/products/workers-ai/" target="_self">x</a><div class="title search-snippet-title line-clamp-1" title="Cloudflare Workers AI &amp; more">t</div><div class="description t-secondary">Pay for inference.</div></div><div class="snippet" data-type="web"><a href="https://developers.cloudflare.com/workers-ai/" target="_self">y</a><div class="title search-snippet-title" title="Overview">t</div></div>"#;
        let results = brave_results(page);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "Cloudflare Workers AI & more");
        assert_eq!(
            results[0].1,
            "https://www.cloudflare.com/products/workers-ai/"
        );
        assert_eq!(results[0].2, "Pay for inference.");
        assert_eq!(results[1].2, "");
        assert!(brave_results("<html>bot check</html>").is_empty());
    }

    #[test]
    fn a_bing_page_gives_title_address_and_snippet() {
        let page = r#"<ol><li class="b_algo"><h2><a href="https://example.com/a?x=1&amp;y=2">First <strong>page</strong></a></h2><div class="b_caption"><p>Snippet one.</p></div></li><li class="b_algo"><h2><a href="https://example.com/b">Second</a></h2></li></ol>"#;
        let results = bing_results(page);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "First page");
        assert_eq!(results[0].1, "https://example.com/a?x=1&y=2");
        assert_eq!(results[0].2, "Snippet one.");
        assert_eq!(results[1].2, "");
    }

    #[test]
    fn a_duckduckgo_page_gives_the_real_address_of_each_result() {
        let page = r#"<a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fc&amp;rut=1">Third</a><a class="result__snippet" href="x">Snippet three.</a><a class="result__a" href="https://example.org/d">Fourth</a>"#;
        let results = duckduckgo_results(page);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1, "https://example.com/c");
        assert_eq!(results[0].2, "Snippet three.");
        assert_eq!(results[1].1, "https://example.org/d");
        assert!(duckduckgo_results("<html>anomaly</html>").is_empty());
    }
}
