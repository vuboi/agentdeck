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
use ag_adapters::agent::permission::{verdict, Action, Mode, Verdict};
use ag_adapters::agent::{
    allow_entry, file_change, summarize_tool_input, AgentEvent, PermissionRequest,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Model calls in one turn before it is stopped, so a model that keeps calling tools cannot run forever.
const MAX_STEPS: usize = 40;
/// A local model that repeats the same tool call is not making progress. Stop the turn early instead of
/// spending the remaining requests until `MAX_STEPS`.
const REPEATED_CALL_LIMIT: usize = 3;
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
fn wire_message(message: &ChatMessage) -> Value {
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
    if message.images.is_empty() {
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
struct Outcome {
    text: String,
    is_error: bool,
    image: Option<String>,
}

fn failed(text: impl Into<String>) -> Outcome {
    Outcome {
        text: text.into(),
        is_error: true,
        image: None,
    }
}

fn ok(text: impl Into<String>) -> Outcome {
    Outcome {
        text: text.into(),
        is_error: false,
        image: None,
    }
}

/// How much of a tool's output the model gets: all of it when it fits, else its start and its end around a marker that says
/// how much was cut (the middle is the part that matters least). Command output keeps mostly its end, where the verdict
/// is; a page or a file keeps mostly its start.
fn trim_output(text: &str, limit: usize, tail_percent: usize) -> String {
    ag_adapters::agent::output::trim_middle(text, limit, tail_percent)
}
/// The end of a command's output is what matters.
const END_HEAVY: usize = 75;
/// A file or a page is read from the top.
const START_HEAVY: usize = 25;
/// A reply of a tool server: no side matters more.
const BALANCED: usize = 50;

fn resolve(cwd: Option<&Path>, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    match cwd {
        Some(dir) if path.is_relative() => dir.join(path),
        _ => path,
    }
}

async fn run_bash(command: &str, cwd: Option<&Path>) -> Outcome {
    #[cfg(windows)]
    let mut process = {
        let mut process = Command::new("powershell");
        process.args(["-NoProfile", "-NonInteractive", "-Command", command]);
        process.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        process
    };
    #[cfg(not(windows))]
    let mut process = {
        let mut process = Command::new("sh");
        process.args(["-c", command]);
        process
    };
    process
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(dir) = cwd.filter(|dir| dir.is_dir()) {
        process.current_dir(dir);
    }
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => return failed(format!("could not start the shell: {error}")),
    };
    // Ends with the app even when the app is killed (servers.rs job).
    if let Some(pid) = child.id() {
        crate::process_guard::adopt_pid(pid);
    }
    let (mut stdout, mut stderr) = (child.stdout.take(), child.stderr.take());
    let collect = async {
        let (mut out, mut err) = (String::new(), String::new());
        if let Some(pipe) = stdout.as_mut() {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes).await;
            out = String::from_utf8_lossy(&bytes).into_owned();
        }
        if let Some(pipe) = stderr.as_mut() {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes).await;
            err = String::from_utf8_lossy(&bytes).into_owned();
        }
        let status = child.wait().await;
        (out, err, status)
    };
    match tokio::time::timeout(Duration::from_secs(120), collect).await {
        Ok((out, err, status)) => {
            let text = [out.trim_end(), err.trim_end()]
                .iter()
                .filter(|part| !part.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join("\n");
            let text = trim_output(&text, OUTPUT_MAX, END_HEAVY);
            match status {
                Ok(status) if status.success() => ok(if text.is_empty() {
                    "(no output)".to_string()
                } else {
                    text
                }),
                Ok(status) => failed(
                    format!("{text}\n(exit code {})", status.code().unwrap_or(-1))
                        .trim()
                        .to_string(),
                ),
                Err(error) => failed(error.to_string()),
            }
        }
        Err(_) => failed("the command ran for more than 120 seconds and was stopped"),
    }
}

const SKIPPED_DIRS: [&str; 7] = [
    ".git",
    "node_modules",
    "target",
    "dist",
    ".svelte-kit",
    ".next",
    "__pycache__",
];

/// A name pattern as a regular expression over `/`-separated relative paths: `**` crosses folders, `*` and `?` do not.
fn glob_regex(pattern: &str) -> Option<regex::Regex> {
    let pattern = pattern.replace('\\', "/");
    let pattern = if pattern.contains('/') {
        pattern
    } else {
        format!("**/{pattern}")
    };
    let mut out = String::from("(?i)^");
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    out.push_str("(?:.*/)?");
                } else {
                    out.push_str(".*");
                }
            }
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            other => out.push_str(&regex::escape(&other.to_string())),
        }
    }
    out.push('$');
    regex::Regex::new(&out).ok()
}

/// The files under `root` (skipping build and dependency folders) with their paths relative to it.
fn files_under(root: &Path) -> impl Iterator<Item = (PathBuf, String)> {
    let base = root.to_path_buf();
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| {
            !(entry.file_type().is_dir()
                && entry.depth() > 0
                && SKIPPED_DIRS.contains(&entry.file_name().to_string_lossy().as_ref()))
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(move |entry| {
            let relative = entry
                .path()
                .strip_prefix(&base)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            (entry.into_path(), relative)
        })
}

fn glob_files(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(pattern) = args["pattern"].as_str() else {
        return failed("pattern is required");
    };
    let Some(matcher) = glob_regex(pattern) else {
        return failed("that pattern is not valid");
    };
    let root = resolve(cwd, args["path"].as_str().unwrap_or("."));
    let mut found: Vec<String> = files_under(&root)
        .filter(|(_, relative)| matcher.is_match(relative))
        .map(|(_, relative)| relative)
        .take(500)
        .collect();
    found.sort();
    ok(if found.is_empty() {
        "no file matches".to_string()
    } else {
        found.join("\n")
    })
}

fn grep_files(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(pattern) = args["pattern"].as_str() else {
        return failed("pattern is required");
    };
    let ignore = args["ignore_case"].as_bool().unwrap_or(false);
    let Ok(matcher) = regex::RegexBuilder::new(pattern)
        .case_insensitive(ignore)
        .build()
    else {
        return failed("that is not a valid regular expression");
    };
    let only = args["glob"].as_str().and_then(glob_regex);
    let root = resolve(cwd, args["path"].as_str().unwrap_or("."));
    let candidates: Vec<(PathBuf, String)> = if root.is_file() {
        vec![(
            root.clone(),
            root.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )]
    } else {
        files_under(&root).collect()
    };
    let mut lines = Vec::new();
    'files: for (path, relative) in candidates {
        if only.as_ref().is_some_and(|only| !only.is_match(&relative)) {
            continue;
        }
        if std::fs::metadata(&path)
            .map(|meta| meta.len() > 2_000_000)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if bytes.iter().take(1024).any(|byte| *byte == 0) {
            continue;
        }
        for (index, line) in String::from_utf8_lossy(&bytes).lines().enumerate() {
            if matcher.is_match(line) {
                lines.push(format!(
                    "{relative}:{}: {}",
                    index + 1,
                    line.trim().chars().take(200).collect::<String>()
                ));
                if lines.len() >= 200 {
                    break 'files;
                }
            }
        }
    }
    ok(if lines.is_empty() {
        "no match".to_string()
    } else {
        lines.join("\n")
    })
}

/// The text of a page: scripts and styles dropped, tags removed, blank runs collapsed.
fn page_text(html: &str) -> String {
    let strip = |text: String, pattern: &str| {
        regex::Regex::new(pattern)
            .map(|re| re.replace_all(&text, " ").into_owned())
            .unwrap_or(text)
    };
    let text = strip(
        html.to_owned(),
        r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>|<noscript[^>]*>.*?</noscript>",
    );
    let text = strip(text, r"(?s)<[^>]+>");
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// `restricted`: a free-tier provider's fetch, which never follows a redirect into this machine or its network.
async fn web_fetch(args: &Value, restricted: bool) -> Outcome {
    let Some(url) = args["url"]
        .as_str()
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
    else {
        return failed("url must start with http:// or https://");
    };
    let redirects = if restricted {
        reqwest::redirect::Policy::custom(|attempt| {
            if crate::privacy::internal_url(attempt.url().as_str()) {
                attempt.error("redirect to an internal address is blocked")
            } else if attempt.previous().len() >= 10 {
                attempt.stop()
            } else {
                attempt.follow()
            }
        })
    } else {
        reqwest::redirect::Policy::default()
    };
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("AgentDeck")
        .redirect(redirects)
        .build()
    {
        Ok(client) => client,
        Err(error) => return failed(error.to_string()),
    };
    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => {
            let html = response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|kind| kind.contains("html"));
            match response.text().await {
                Ok(text) => ok(trim_output(
                    &if html { page_text(&text) } else { text },
                    OUTPUT_MAX,
                    START_HEAVY,
                )),
                Err(error) => failed(error.to_string()),
            }
        }
        Ok(response) => failed(format!("the server answered {}", response.status())),
        Err(error) => failed(error.to_string()),
    }
}

fn list_dir(args: &Value, cwd: Option<&Path>) -> Outcome {
    let dir = resolve(cwd, args["path"].as_str().unwrap_or("."));
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) => return failed(format!("could not list {}: {error}", dir.display())),
    };
    let mut rows: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => format!("{name}/"),
                Ok(meta) => format!("{name}  ({} bytes)", meta.len()),
                Err(_) => name,
            }
        })
        .collect();
    rows.sort();
    ok(if rows.is_empty() {
        "(empty folder)".to_string()
    } else {
        rows.join("\n")
    })
}

fn make_dir(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(path) = args["path"].as_str() else {
        return failed("path is required");
    };
    match std::fs::create_dir_all(resolve(cwd, path)) {
        Ok(()) => ok(format!("Folder ready: {path}")),
        Err(error) => failed(format!("could not create {path}: {error}")),
    }
}

/// Copy or move: refuses to overwrite, so nothing is lost by a slip.
fn transfer(args: &Value, cwd: Option<&Path>, moving: bool) -> Outcome {
    let (Some(from), Some(to)) = (args["from"].as_str(), args["to"].as_str()) else {
        return failed("from and to are required");
    };
    let (source, target) = (resolve(cwd, from), resolve(cwd, to));
    if !source.exists() {
        return failed(format!("{from} does not exist"));
    }
    if target.exists() {
        return failed(format!(
            "{to} already exists: pick another name or Delete it first"
        ));
    }
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let done = if moving {
        std::fs::rename(&source, &target)
    } else if source.is_dir() {
        copy_dir(&source, &target)
    } else {
        std::fs::copy(&source, &target).map(|_| ())
    };
    match done {
        Ok(()) => ok(format!(
            "{} {from} to {to}",
            if moving { "Moved" } else { "Copied" }
        )),
        Err(error) => failed(error.to_string()),
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.filter_map(Result::ok) {
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Deletes a file or folder, but only inside the project folder.
fn delete_path(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(path) = args["path"].as_str() else {
        return failed("path is required");
    };
    let target = resolve(cwd, path);
    let (Ok(real), Some(Ok(root))) = (target.canonicalize(), cwd.map(Path::canonicalize)) else {
        return failed(format!(
            "{path} does not exist, or there is no project folder"
        ));
    };
    if real == root || !real.starts_with(&root) {
        return failed("only files and folders inside the project folder can be deleted");
    }
    let done = if real.is_dir() {
        std::fs::remove_dir_all(&real)
    } else {
        std::fs::remove_file(&real)
    };
    match done {
        Ok(()) => ok(format!("Deleted {path}")),
        Err(error) => failed(error.to_string()),
    }
}

/// A web search through DuckDuckGo's plain HTML page: title, address and snippet of the first results.
async fn web_search(args: &Value) -> Outcome {
    let Some(query) = args["query"]
        .as_str()
        .filter(|query| !query.trim().is_empty())
    else {
        return failed("query is required");
    };
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("Mozilla/5.0 AgentDeck")
        .build()
    {
        Ok(client) => client,
        Err(error) => return failed(error.to_string()),
    };
    let page = match client
        .get(crate::endpoints::ENDPOINTS.web.search.as_str())
        .query(&[("q", query)])
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response.text().await.unwrap_or_default(),
        Ok(response) => return failed(format!("the search answered {}", response.status())),
        Err(error) => return failed(error.to_string()),
    };
    let (Ok(link), Ok(snippet)) = (
        regex::Regex::new(r#"(?s)class="result__a"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#),
        regex::Regex::new(r#"(?s)class="result__snippet"[^>]*>(.*?)</a>"#),
    ) else {
        return failed("search is unavailable");
    };
    let clean = |html: &str| page_text(html).replace('\n', " ");
    let snippets: Vec<String> = snippet
        .captures_iter(&page)
        .map(|found| clean(&found[1]))
        .collect();
    let mut rows = Vec::new();
    for (index, found) in link.captures_iter(&page).take(8).enumerate() {
        let href = found[1].replace("&amp;", "&");
        let href = if href.starts_with("//") {
            format!("https:{href}")
        } else {
            href
        };
        // The result address is wrapped in a redirect: `?uddg=<the real address>`.
        let target = tauri::Url::parse(&href)
            .ok()
            .and_then(|url| {
                url.query_pairs()
                    .find(|(key, _)| key == "uddg")
                    .map(|(_, value)| value.into_owned())
            })
            .unwrap_or(href);
        rows.push(format!(
            "{}. {}\n   {target}\n   {}",
            index + 1,
            clean(&found[2]),
            snippets.get(index).cloned().unwrap_or_default()
        ));
    }
    ok(if rows.is_empty() {
        "no results".to_string()
    } else {
        rows.join("\n")
    })
}

/// The file change a Write or Edit makes. A Write over an existing file shows what it replaces, so the diff is
/// the real one and not "every line added".
fn change_for(
    name: &str,
    args: &Value,
    cwd: Option<&Path>,
) -> Option<ag_adapters::agent::FileChange> {
    let mut change = file_change(name, args)?;
    if name == "Write" {
        if let Ok(old) =
            std::fs::read_to_string(resolve(cwd, args["file_path"].as_str().unwrap_or_default()))
        {
            change.removed = old.lines().count();
            change.before = old.chars().take(3000).collect();
        }
    }
    Some(change)
}

fn read_file(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(path) = args["file_path"].as_str() else {
        return failed("file_path is required");
    };
    let text = match std::fs::read(resolve(cwd, path)) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) => return failed(format!("could not read {path}: {error}")),
    };
    let first = args["offset"].as_u64().unwrap_or(1).max(1) as usize;
    let limit = args["limit"].as_u64().unwrap_or(2000) as usize;
    let numbered: Vec<String> = text
        .lines()
        .enumerate()
        .skip(first - 1)
        .take(limit)
        .map(|(index, line)| format!("{:>6}\t{line}", index + 1))
        .collect();
    ok(if numbered.is_empty() {
        "(empty)".to_string()
    } else {
        trim_output(&numbered.join("\n"), OUTPUT_MAX * 3, START_HEAVY)
    })
}

fn write_file(args: &Value, cwd: Option<&Path>) -> Outcome {
    let (Some(path), Some(content)) = (args["file_path"].as_str(), args["content"].as_str()) else {
        return failed("file_path and content are required");
    };
    let target = resolve(cwd, path);
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let existed = target.exists();
    match std::fs::write(&target, content) {
        Ok(()) => ok(format!(
            "{} {} lines {} {path}",
            if existed { "Updated" } else { "Created" },
            content.lines().count(),
            if existed { "in" } else { "in new file" }
        )),
        Err(error) => failed(format!("could not write {path}: {error}")),
    }
}

/// A key that names one file however it was written (relative, with `./`, back slashes, other case on Windows).
fn path_key(cwd: Option<&Path>, path: &str) -> String {
    let full = resolve(cwd, path);
    let full = std::fs::canonicalize(&full).unwrap_or(full);
    full.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("//?/")
        .to_lowercase()
}

/// Lines of `text` around `at` (0-based), numbered, for showing the model what is really there.
fn numbered_around(text: &str, at: usize, before: usize, after: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let from = at.saturating_sub(before);
    let to = (at + after + 1).min(lines.len());
    (from..to)
        .map(|index| format!("{:>6}\t{}", index + 1, lines[index]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Why an Edit did not find its text: the nearest place in the file, so the next try can copy it exactly.
fn not_found(text: &str, old: &str, path: &str) -> String {
    let wanted: Vec<&str> = old
        .lines()
        .map(str::trim)
        .filter(|line| line.len() > 3)
        .collect();
    let lines: Vec<&str> = text.lines().collect();
    let hit = wanted
        .iter()
        .find_map(|line| lines.iter().position(|candidate| candidate.trim() == *line))
        .or_else(|| {
            wanted
                .iter()
                .find_map(|line| lines.iter().position(|candidate| candidate.contains(*line)))
        });
    match hit {
        Some(at) => format!("old_string was not found in {path}: it must match the file exactly, spaces, tabs and line breaks included. The closest place is at line {}:\n{}\nCopy the text from here (Read the lines if you need more) and try again.", at + 1, numbered_around(text, at, 2, 8)),
        None => format!("old_string was not found in {path}, and none of its lines is in the file. Read the file (or code_outline it), then copy the exact text you want to replace."),
    }
}

/// The Plan tool: what will change, and where. Returns the plan as the model should see it, and the files it names.
fn plan_tool(
    args: &Value,
    cwd: Option<&Path>,
) -> (Outcome, Option<std::collections::HashSet<String>>) {
    let list = |name: &str| {
        args[name]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let (files, steps) = (list("files"), list("steps"));
    if files.is_empty() || steps.is_empty() {
        return (failed("Give the files you will change (find them with code_search, Grep or Glob first) and the steps."), None);
    }
    let mut keys = std::collections::HashSet::new();
    let mut out = format!(
        "Plan recorded: {}\nFiles:",
        args["goal"].as_str().unwrap_or_default()
    );
    for file in &files {
        keys.insert(path_key(cwd, file));
        out.push_str(&format!(
            "\n- {file} ({})",
            if resolve(cwd, file).exists() {
                "exists: Read it before you edit it"
            } else {
                "will be created"
            }
        ));
    }
    out.push_str("\nSteps:");
    for (index, step) in steps.iter().enumerate() {
        out.push_str(&format!("\n{}. {step}", index + 1));
    }
    if let Some(checks) = args["checks"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
    {
        out.push_str(&format!("\nCheck with: {checks}"));
    }
    out.push_str("\nDo step 1 now. After the last step, run the check and look at the result before you answer.");
    (ok(out), Some(keys))
}

/// The latest version of a package, from its registry.
async fn package_info(args: &Value) -> Outcome {
    let Some(name) = args["name"]
        .as_str()
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.len() < 120 && !name.contains(char::is_whitespace))
    else {
        return failed("name is required");
    };
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("AgentDeck")
        .build()
    {
        Ok(client) => client,
        Err(error) => return failed(error.to_string()),
    };
    let get = |url: String| {
        let client = client.clone();
        async move {
            client
                .get(url)
                .send()
                .await
                .map_err(|error| error.to_string())?
                .error_for_status()
                .map_err(|error| error.to_string())?
                .json::<Value>()
                .await
                .map_err(|error| error.to_string())
        }
    };
    let text = |value: &Value| value.as_str().unwrap_or_default().to_owned();
    match args["ecosystem"].as_str().unwrap_or("npm") {
        "crates" | "cargo" | "rust" => {
            match get(format!("https://crates.io/api/v1/crates/{name}")).await {
                Ok(json) => {
                    let krate = &json["crate"];
                    ok(format!(
                        "{name} (crates.io)\nlatest stable: {}\nnewest: {}\n{}\ndocs: {}\nhome: {}",
                        text(&krate["max_stable_version"]),
                        text(&krate["newest_version"]),
                        text(&krate["description"]),
                        text(&krate["documentation"]),
                        text(&krate["homepage"])
                    ))
                }
                Err(error) => failed(format!("no answer for {name}: {error}")),
            }
        }
        "pypi" | "python" => match get(format!("https://pypi.org/pypi/{name}/json")).await {
            Ok(json) => ok(format!(
                "{name} (PyPI)\nlatest: {}\n{}\nhome: {}\nrequires python: {}",
                text(&json["info"]["version"]),
                text(&json["info"]["summary"]),
                text(&json["info"]["home_page"]),
                text(&json["info"]["requires_python"])
            )),
            Err(error) => failed(format!("no answer for {name}: {error}")),
        },
        _ => match get(format!(
            "https://registry.npmjs.org/{}/latest",
            name.replace('/', "%2F")
        ))
        .await
        {
            Ok(json) => {
                let tags = get(format!(
                    "https://registry.npmjs.org/-/package/{}/dist-tags",
                    name.replace('/', "%2F")
                ))
                .await
                .map(|tags| tags.to_string())
                .unwrap_or_default();
                let deprecated = json["deprecated"]
                    .as_str()
                    .map(|why| format!("\nDEPRECATED: {why}"))
                    .unwrap_or_default();
                ok(format!(
                    "{name} (npm)\nlatest: {}\ntags: {tags}\n{}\nhome: {}{deprecated}",
                    text(&json["version"]),
                    text(&json["description"]),
                    text(&json["homepage"])
                ))
            }
            Err(error) => failed(format!("no answer for {name}: {error}")),
        },
    }
}

/// What the project says about itself, for the model: the versions of its main libraries (so it writes for those, not for
/// the ones it remembers), its scripts, and its own rules file.
fn project_brief(cwd: Option<&Path>) -> String {
    let Some(dir) = cwd else { return String::new() };
    let mut out = String::new();
    if let Ok(text) = std::fs::read_to_string(dir.join("package.json")) {
        if let Ok(json) = serde_json::from_str::<Value>(&text) {
            const MAIN: [&str; 24] = [
                "@angular/core",
                "@angular/material",
                "svelte",
                "@sveltejs/kit",
                "react",
                "react-dom",
                "next",
                "vue",
                "nuxt",
                "solid-js",
                "tailwindcss",
                "vite",
                "typescript",
                "rxjs",
                "zod",
                "@tauri-apps/api",
                "@ngrx/store",
                "@tanstack/query-core",
                "pinia",
                "express",
                "nestjs",
                "@nestjs/core",
                "prisma",
                "drizzle-orm",
            ];
            let mut found = Vec::new();
            for section in ["dependencies", "devDependencies"] {
                for (name, version) in json[section].as_object().into_iter().flatten() {
                    if MAIN.contains(&name.as_str()) || name.starts_with("@shival99/") {
                        found.push(format!("{name}@{}", version.as_str().unwrap_or_default()));
                    }
                }
            }
            if !found.is_empty() {
                out.push_str(&format!("\n\nThe project's own versions (write code for these, not for older ones you may remember): {}.", found.join(", ")));
            }
            let scripts: Vec<&str> = json["scripts"]
                .as_object()
                .into_iter()
                .flat_map(|map| map.keys().map(String::as_str))
                .take(16)
                .collect();
            if !scripts.is_empty() {
                out.push_str(&format!("\npackage.json scripts: {}.", scripts.join(", ")));
            }
        }
    }
    for file in ["AGENTS.md", "CLAUDE.md"] {
        if let Ok(text) = std::fs::read_to_string(dir.join(file)) {
            let rules: String = text.chars().take(4500).collect();
            out.push_str(&format!(
                "\n\nThe project's rules ({file}); follow them:\n{rules}"
            ));
            break;
        }
    }
    out
}

fn edit_file(args: &Value, cwd: Option<&Path>) -> Outcome {
    let (Some(path), Some(old), Some(new)) = (
        args["file_path"].as_str(),
        args["old_string"].as_str(),
        args["new_string"].as_str(),
    ) else {
        return failed("file_path, old_string and new_string are required");
    };
    let target = resolve(cwd, path);
    let text = match std::fs::read_to_string(&target) {
        Ok(text) => text,
        Err(error) => return failed(format!("could not read {path}: {error}")),
    };
    // A file with Windows line ends, and a model that wrote its text with plain ones: the same text.
    let crlf = text.contains("\r\n");
    let fit = |piece: &str| {
        if crlf && !piece.contains("\r\n") {
            piece.replace('\n', "\r\n")
        } else {
            piece.to_owned()
        }
    };
    let (old, new) = (fit(old), fit(new));
    let (old, new) = (old.as_str(), new.as_str());
    match text.matches(old).count() {
        0 => failed(not_found(&text, old, path)),
        1 => {
            let line = text[..text.find(old).unwrap_or(0)].matches('\n').count();
            let updated = text.replacen(old, new, 1);
            match std::fs::write(&target, &updated) {
                // The result is shown back, so the model sees where its text landed and what is around it.
                Ok(()) => ok(format!(
                    "Edited {path} at line {}. It now reads:\n{}",
                    line + 1,
                    numbered_around(&updated, line, 3, new.lines().count() + 2)
                )),
                Err(error) => failed(format!("could not write {path}: {error}")),
            }
        }
        count if args["replace_all"].as_bool() == Some(true) => {
            match std::fs::write(&target, text.replace(old, new)) {
                Ok(()) => ok(format!("Edited {path}: {count} replacements")),
                Err(error) => failed(format!("could not write {path}: {error}")),
            }
        }
        count => failed(format!(
            "old_string appears {count} times: add more surrounding text, or set replace_all"
        )),
    }
}

/// Calls one AgentDeck browser tool through the MCP server the app runs for itself.
async fn browser_call(app: &AppHandle, chat: &str, name: &str, args: &Value) -> Outcome {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }).to_string();
    let Some(answer) = crate::mcp::handle(app, &request, chat).await else {
        return failed("the browser did not answer");
    };
    let Ok(answer) = serde_json::from_str::<Value>(&answer) else {
        return failed("the browser answered with something unreadable");
    };
    let result = &answer["result"];
    let mut text = Vec::new();
    let mut image = None;
    for part in result["content"].as_array().into_iter().flatten() {
        match part["type"].as_str() {
            Some("text") => text.push(part["text"].as_str().unwrap_or_default().to_owned()),
            Some("image") => {
                image = Some(format!(
                    "data:{};base64,{}",
                    part["mimeType"].as_str().unwrap_or("image/png"),
                    part["data"].as_str().unwrap_or_default()
                ));
            }
            _ => {}
        }
    }
    let mut text = text.join("\n");
    if text.is_empty() && image.is_some() {
        text = "Screenshot taken.".into();
    }
    Outcome {
        text: trim_output(&text, OUTPUT_MAX, BALANCED),
        is_error: result["isError"].as_bool().unwrap_or(false),
        image,
    }
}

enum Decision {
    Allow,
    Deny(&'static str),
    Ask,
}

/// Whether a tool call may run now, under the permission mode and what the user approved before.
/// `external`: the tool comes from an MCP server the user set up (AgentDeck's own browser tools are not external).
fn decide(turn: &Turn, name: &str, args: &Value, external: bool) -> Decision {
    // Whatever the model asks for, a restricted provider runs nothing that touches this machine.
    if turn.restricted && (external || !RESTRICTED_TOOLS.contains(&name)) {
        return Decision::Deny("this provider has no access to the project or the machine: say what is needed and another agent will do it.");
    }
    if turn.restricted {
        // Its web tools must not carry a secret out in a query, nor reach this machine or its network.
        if name == "WebFetch"
            && args["url"]
                .as_str()
                .is_some_and(crate::privacy::internal_url)
        {
            return Decision::Deny("internal addresses (localhost, private IPs, .internal hosts) are blocked for this provider.");
        }
        if crate::privacy::private_content(&args.to_string()).is_some() {
            return Decision::Deny("the call holds a secret, an environment variable or an internal address: blocked for this provider.");
        }
    }
    // A guard can only refuse and runs before the mode and before any earlier "always allow".
    if name == "Bash" {
        if let Some(reason) = ag_adapters::agent::permission::guard_shell(args["command"].as_str().unwrap_or_default()) {
            return Decision::Deny(reason);
        }
    }
    let mode = Mode::parse(turn.permission_mode.as_deref());
    let approved = turn
        .allowed
        .iter()
        .any(|entry| entry == name || *entry == allow_entry(name, args));
    let (action, refusal) = match name {
        "Read" | "Glob" | "Grep" | "WebFetch" | "WebSearch" | "Skill" | "LS" => return Decision::Allow,
        "Write" | "Edit" | "MkDir" | "Move" | "Copy" => (
            Action::Write,
            "plan mode: nothing may be changed. Describe the plan instead.",
        ),
        "Bash" | "Delete" => (
            Action::Shell,
            "plan mode: commands may not run. Describe the plan instead.",
        ),
        // External tools from the user's MCP servers.
        _ if external => (Action::Other, "plan mode: external tools may not run."),
        // The browser tools of AgentDeck itself.
        _ => return Decision::Allow,
    };
    match verdict(mode, action) {
        Verdict::Approve => Decision::Allow,
        Verdict::Refuse => Decision::Deny(refusal),
        Verdict::Ask if approved => Decision::Allow,
        Verdict::Ask => Decision::Ask,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// One model call

#[derive(Default)]
struct Call {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct Step {
    /// Why the model stopped: `stop`, `tool_calls`, `length`…
    finish: String,
    text: String,
    calls: Vec<Call>,
    /// Prompt tokens not served from the provider's prompt cache.
    input_tokens: u64,
    /// Prompt tokens the provider read from its cache (billed at a fraction, and a sign the prefix stayed stable).
    cached_tokens: u64,
    output_tokens: u64,
}

/// Reads one `data:` line of the stream: emits text and reasoning, collects the tool calls.
fn read_chunk(data: &str, step: &mut Step, emit: &mut impl FnMut(AgentEvent)) {
    let Ok(json) = serde_json::from_str::<Value>(data) else {
        return;
    };
    let delta = &json["choices"][0]["delta"];
    for key in ["reasoning_content", "reasoning"] {
        if let Some(part) = delta[key].as_str().filter(|part| !part.is_empty()) {
            emit(AgentEvent::Thinking {
                text: part.to_owned(),
            });
            break;
        }
    }
    if let Some(part) = delta["content"].as_str().filter(|part| !part.is_empty()) {
        step.text.push_str(part);
        emit(AgentEvent::Delta {
            text: part.to_owned(),
        });
    }
    if let Some(finish) = json["choices"][0]["finish_reason"].as_str() {
        step.finish = finish.to_owned();
    }
    for piece in delta["tool_calls"].as_array().into_iter().flatten() {
        let index = piece["index"].as_u64().unwrap_or(0) as usize;
        while step.calls.len() <= index {
            step.calls.push(Call::default());
        }
        let call = &mut step.calls[index];
        if let Some(id) = piece["id"].as_str().filter(|id| !id.is_empty()) {
            call.id = id.to_owned();
        }
        if let Some(name) = piece["function"]["name"].as_str() {
            call.name.push_str(name);
        }
        if let Some(arguments) = piece["function"]["arguments"].as_str() {
            call.arguments.push_str(arguments);
        }
    }
    if let Some(usage) = json.get("usage").filter(|usage| usage.is_object()) {
        let prompt = usage["prompt_tokens"].as_u64().unwrap_or(0);
        // OpenAI names the cached part `prompt_tokens_details.cached_tokens`; DeepSeek `prompt_cache_hit_tokens`.
        let cached = usage["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .or_else(|| usage["prompt_cache_hit_tokens"].as_u64())
            .unwrap_or(0)
            .min(prompt);
        step.input_tokens = prompt - cached;
        step.cached_tokens = cached;
        step.output_tokens = usage["completion_tokens"].as_u64().unwrap_or(0);
    }
}

/// Sends the conversation and streams the answer. `Err` is a message for the user.
async fn model_call(
    client: &reqwest::Client,
    turn: &Turn,
    messages: &[Value],
    tools: &[Value],
    emit: &mut impl FnMut(AgentEvent),
) -> Result<Step, String> {
    let mut body = json!({
        "model": turn.model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": messages,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.to_vec());
    }
    // Servers differ in how they take this, and one ignores what it does not know: `reasoning_effort` (LM Studio,
    // OpenAI-style, Ollama), `chat_template_kwargs` (llama.cpp, vLLM) and `think` (Ollama).
    match turn.effort.as_deref() {
        Some("off") => {
            body["reasoning_effort"] = json!("none");
            body["chat_template_kwargs"] = json!({ "enable_thinking": false });
            body["think"] = json!(false);
        }
        Some(level @ ("low" | "medium" | "high")) => body["reasoning_effort"] = json!(level),
        Some(_) => body["reasoning_effort"] = json!("high"),
        None => {}
    }
    let mut request = client
        .post(chat_url(&turn.kind, &turn.base_url))
        .json(&body);
    if let Some(key) = turn.api_key.as_deref().filter(|key| !key.is_empty()) {
        request = request.bearer_auth(key);
    }
    let mut response = match request.send().await {
        Ok(response) => response,
        Err(error) if error.is_connect() => {
            return Err("could not connect: is the server running?".into())
        }
        Err(error) => return Err(error.to_string()),
    };
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<Value>(&detail)
            .ok()
            .and_then(|json| json["error"]["message"].as_str().map(str::to_owned))
            .unwrap_or(detail);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            format!("the server answered {status}")
        } else {
            format!("{status}: {detail}")
        });
    }
    let mut step = Step::default();
    let mut pending = String::new();
    'stream: loop {
        // A server that stops sending (a stuck model, a dropped connection) must not leave the chat waiting for ever.
        let next = match tokio::time::timeout(STALL, response.chunk()).await {
            Ok(next) => next,
            Err(_) => {
                return Err(format!(
                    "the model sent nothing for {} seconds and was stopped",
                    STALL.as_secs()
                ))
            }
        };
        match next {
            Ok(Some(bytes)) => {
                pending.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(end) = pending.find('\n') {
                    let line: String = pending.drain(..=end).collect();
                    let Some(data) = line.trim().strip_prefix("data:") else {
                        continue;
                    };
                    let data = data.trim();
                    if data == "[DONE]" {
                        break 'stream;
                    }
                    read_chunk(data, &mut step, emit);
                }
            }
            Ok(None) => break,
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(step)
}

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
         Browser: the browser_* tools drive the Preview panel, the page the user is looking at. To run a dev server use browser_serve (a server started with Bash dies when the command ends), then browser_open if needed. \
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
    if turn.kind == "anthropic" {
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
    messages.extend(history.iter().map(wire_message));

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
    // A model that spends its whole answer on thinking (some local ones do, with big prompts): it is asked again, then asked
    // again with thinking off, before the turn is called a failure.
    let mut silent = 0;
    for step_number in 0..MAX_STEPS {
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
                messages.push(json!({ "role": "assistant", "content": step.text }));
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
        messages.push(json!({
            "role": "assistant",
            "content": if step.text.is_empty() { Value::Null } else { Value::String(step.text.clone()) },
            "tool_calls": calls.iter().zip(&ids).map(|(call, id)| json!({ "id": id, "type": "function", "function": { "name": call.name, "arguments": if call.arguments.is_empty() { "{}" } else { call.arguments.as_str() } } })).collect::<Vec<_>>(),
        }));

        let mut denied = Vec::new();
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
                name: shown,
                input: summarize_tool_input(&call.name, &args),
                change: change_for(&call.name, &args, turn.cwd.as_deref()),
                background: false,
            });
            let outcome = if !args.is_object() {
                failed("the arguments were not valid JSON")
            } else {
                match decide(&turn, &call.name, &args, user_tool.is_some()) {
                    Decision::Deny(reason) => failed(reason),
                    Decision::Ask => {
                        denied.push(PermissionRequest { id: id.clone(), tool: call.name.clone(), summary: summarize_tool_input(&call.name, &args), allow: allow_entry(&call.name, &args) });
                        failed("this needs the user's approval; it was not run")
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
            emit(AgentEvent::ToolResult {
                id: id.clone(),
                output: outcome.text.clone(),
                is_error: outcome.is_error,
                image: outcome.image.clone(),
                change: None,
            });
            messages.push(json!({ "role": "tool", "tool_call_id": id, "content": outcome.text }));
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

    fn turn(mode: Option<&str>, allowed: &[&str]) -> Turn {
        Turn {
            kind: "openai".into(),
            base_url: String::new(),
            api_key: None,
            model: String::new(),
            cwd: None,
            permission_mode: mode.map(str::to_owned),
            allowed: allowed.iter().map(|s| s.to_string()).collect(),
            chat: String::new(),
            effort: None,
            restricted: false,
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
            Decision::Allow | Decision::Ask
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
    fn chunks_give_text_reasoning_tool_calls_and_usage() {
        let mut step = Step::default();
        let mut events = Vec::new();
        let mut emit = |event| events.push(event);
        read_chunk(
            r#"{"choices":[{"delta":{"reasoning_content":"hm","content":"hi"}}]}"#,
            &mut step,
            &mut emit,
        );
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
        let build = json!({ "command": "rm -rf target" });
        for mode in [None, Some("acceptEdits"), Some("auto"), Some("bypassPermissions")] {
            assert!(matches!(decide(&turn(mode, &[]), "Bash", &wipe, false), Decision::Deny(_)), "{mode:?}");
        }
        // Not even a command the user allowed before.
        assert!(matches!(decide(&turn(None, &["Bash(rm -rf /)"]), "Bash", &wipe, false), Decision::Deny(_)));
        // An ordinary command is still the mode's business.
        assert!(matches!(decide(&turn(Some("bypassPermissions"), &[]), "Bash", &build, false), Decision::Allow));
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
}
