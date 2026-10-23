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
use ag_adapters::agent::{allow_entry, file_change, summarize_tool_input, AgentEvent, PermissionRequest};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Model calls in one turn before it is stopped, so a model that keeps calling tools cannot run forever.
const MAX_STEPS: usize = 40;
/// Characters of a tool's output kept for the model.
const OUTPUT_MAX: usize = 20_000;
/// How long the model may be silent in the middle of an answer. A big local model can take a while to start.
const STALL: Duration = Duration::from_secs(240);
/// How long one browser or MCP tool call may take.
const TOOL_LIMIT: Duration = Duration::from_secs(150);
const MCP_PREFIX: &str = "mcp__agentdeck__";

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
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
}

/// The address of the chat route for a provider kind.
fn chat_url(kind: &str, base: &str) -> String {
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
        function("WebFetch", "Downloads a web page or API address and returns its text (HTML is reduced to its text).", json!({ "url": { "type": "string" } }), &["url"]),
        function("Skill", "Loads the full instructions of one of the skills listed in the system message. Call it when the task matches a skill, then follow what it says.", json!({ "name": { "type": "string", "description": "the skill's name" } }), &["name"]),
    ]
}

/// The AgentDeck browser tools, asked of the MCP server the app runs for itself, as OpenAI functions.
async fn browser_tools(app: &AppHandle, chat: &str) -> Vec<Value> {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }).to_string();
    let Some(answer) = crate::mcp::handle(app, &request, chat).await else { return Vec::new() };
    let Ok(answer) = serde_json::from_str::<Value>(&answer) else { return Vec::new() };
    answer["result"]["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|tool| function(tool["name"].as_str().unwrap_or_default(), tool["description"].as_str().unwrap_or_default(), tool["inputSchema"]["properties"].clone(), &tool["inputSchema"]["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>()))
        .collect()
}

/// What running a tool gave: the text for the model, whether it failed, and a picture it returned.
struct Outcome {
    text: String,
    is_error: bool,
    image: Option<String>,
}

fn failed(text: impl Into<String>) -> Outcome {
    Outcome { text: text.into(), is_error: true, image: None }
}

fn ok(text: impl Into<String>) -> Outcome {
    Outcome { text: text.into(), is_error: false, image: None }
}

/// The end of `text` when it is longer than the limit (the end of a command's output is what matters).
fn tail(text: &str, limit: usize) -> String {
    let count = text.chars().count();
    if count <= limit {
        return text.to_owned();
    }
    let kept: String = text.chars().skip(count - limit).collect();
    format!("…(cut {} characters)\n{kept}", count - limit)
}

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
    process.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    if let Some(dir) = cwd.filter(|dir| dir.is_dir()) {
        process.current_dir(dir);
    }
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => return failed(format!("could not start the shell: {error}")),
    };
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
            let text = [out.trim_end(), err.trim_end()].iter().filter(|part| !part.is_empty()).copied().collect::<Vec<_>>().join("\n");
            let text = tail(&text, OUTPUT_MAX);
            match status {
                Ok(status) if status.success() => ok(if text.is_empty() { "(no output)".to_string() } else { text }),
                Ok(status) => failed(format!("{text}\n(exit code {})", status.code().unwrap_or(-1)).trim().to_string()),
                Err(error) => failed(error.to_string()),
            }
        }
        Err(_) => failed("the command ran for more than 120 seconds and was stopped"),
    }
}

const SKIPPED_DIRS: [&str; 7] = [".git", "node_modules", "target", "dist", ".svelte-kit", ".next", "__pycache__"];

/// A name pattern as a regular expression over `/`-separated relative paths: `**` crosses folders, `*` and `?` do not.
fn glob_regex(pattern: &str) -> Option<regex::Regex> {
    let pattern = pattern.replace('\\', "/");
    let pattern = if pattern.contains('/') { pattern } else { format!("**/{pattern}") };
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
        .filter_entry(|entry| !(entry.file_type().is_dir() && entry.depth() > 0 && SKIPPED_DIRS.contains(&entry.file_name().to_string_lossy().as_ref())))
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(move |entry| {
            let relative = entry.path().strip_prefix(&base).unwrap_or(entry.path()).to_string_lossy().replace('\\', "/");
            (entry.into_path(), relative)
        })
}

fn glob_files(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(pattern) = args["pattern"].as_str() else { return failed("pattern is required") };
    let Some(matcher) = glob_regex(pattern) else { return failed("that pattern is not valid") };
    let root = resolve(cwd, args["path"].as_str().unwrap_or("."));
    let mut found: Vec<String> = files_under(&root).filter(|(_, relative)| matcher.is_match(relative)).map(|(_, relative)| relative).take(500).collect();
    found.sort();
    ok(if found.is_empty() { "no file matches".to_string() } else { found.join("\n") })
}

fn grep_files(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(pattern) = args["pattern"].as_str() else { return failed("pattern is required") };
    let ignore = args["ignore_case"].as_bool().unwrap_or(false);
    let Ok(matcher) = regex::RegexBuilder::new(pattern).case_insensitive(ignore).build() else { return failed("that is not a valid regular expression") };
    let only = args["glob"].as_str().and_then(glob_regex);
    let root = resolve(cwd, args["path"].as_str().unwrap_or("."));
    let candidates: Vec<(PathBuf, String)> = if root.is_file() {
        vec![(root.clone(), root.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default())]
    } else {
        files_under(&root).collect()
    };
    let mut lines = Vec::new();
    'files: for (path, relative) in candidates {
        if only.as_ref().is_some_and(|only| !only.is_match(&relative)) {
            continue;
        }
        if std::fs::metadata(&path).map(|meta| meta.len() > 2_000_000).unwrap_or(true) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else { continue };
        if bytes.iter().take(1024).any(|byte| *byte == 0) {
            continue;
        }
        for (index, line) in String::from_utf8_lossy(&bytes).lines().enumerate() {
            if matcher.is_match(line) {
                lines.push(format!("{relative}:{}: {}", index + 1, line.trim().chars().take(200).collect::<String>()));
                if lines.len() >= 200 {
                    break 'files;
                }
            }
        }
    }
    ok(if lines.is_empty() { "no match".to_string() } else { lines.join("\n") })
}

/// The text of a page: scripts and styles dropped, tags removed, blank runs collapsed.
fn page_text(html: &str) -> String {
    let strip = |text: String, pattern: &str| regex::Regex::new(pattern).map(|re| re.replace_all(&text, " ").into_owned()).unwrap_or(text);
    let text = strip(html.to_owned(), r"(?is)<script[^>]*>.*?</script>|<style[^>]*>.*?</style>|<noscript[^>]*>.*?</noscript>");
    let text = strip(text, r"(?s)<[^>]+>");
    let text = text.replace("&nbsp;", " ").replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"");
    text.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("\n")
}

async fn web_fetch(args: &Value) -> Outcome {
    let Some(url) = args["url"].as_str().filter(|url| url.starts_with("http://") || url.starts_with("https://")) else { return failed("url must start with http:// or https://") };
    let client = match reqwest::Client::builder().timeout(Duration::from_secs(30)).user_agent("AgentDeck").build() {
        Ok(client) => client,
        Err(error) => return failed(error.to_string()),
    };
    match client.get(url).send().await {
        Ok(response) if response.status().is_success() => {
            let html = response.headers().get("content-type").and_then(|value| value.to_str().ok()).is_some_and(|kind| kind.contains("html"));
            match response.text().await {
                Ok(text) => ok(tail(&if html { page_text(&text) } else { text }, OUTPUT_MAX)),
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
    ok(if rows.is_empty() { "(empty folder)".to_string() } else { rows.join("\n") })
}

fn make_dir(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(path) = args["path"].as_str() else { return failed("path is required") };
    match std::fs::create_dir_all(resolve(cwd, path)) {
        Ok(()) => ok(format!("Folder ready: {path}")),
        Err(error) => failed(format!("could not create {path}: {error}")),
    }
}

/// Copy or move: refuses to overwrite, so nothing is lost by a slip.
fn transfer(args: &Value, cwd: Option<&Path>, moving: bool) -> Outcome {
    let (Some(from), Some(to)) = (args["from"].as_str(), args["to"].as_str()) else { return failed("from and to are required") };
    let (source, target) = (resolve(cwd, from), resolve(cwd, to));
    if !source.exists() {
        return failed(format!("{from} does not exist"));
    }
    if target.exists() {
        return failed(format!("{to} already exists: pick another name or Delete it first"));
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
        Ok(()) => ok(format!("{} {from} to {to}", if moving { "Moved" } else { "Copied" })),
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
    let Some(path) = args["path"].as_str() else { return failed("path is required") };
    let target = resolve(cwd, path);
    let (Ok(real), Some(Ok(root))) = (target.canonicalize(), cwd.map(Path::canonicalize)) else { return failed(format!("{path} does not exist, or there is no project folder")) };
    if real == root || !real.starts_with(&root) {
        return failed("only files and folders inside the project folder can be deleted");
    }
    let done = if real.is_dir() { std::fs::remove_dir_all(&real) } else { std::fs::remove_file(&real) };
    match done {
        Ok(()) => ok(format!("Deleted {path}")),
        Err(error) => failed(error.to_string()),
    }
}

/// A web search through DuckDuckGo's plain HTML page: title, address and snippet of the first results.
async fn web_search(args: &Value) -> Outcome {
    let Some(query) = args["query"].as_str().filter(|query| !query.trim().is_empty()) else { return failed("query is required") };
    let client = match reqwest::Client::builder().timeout(Duration::from_secs(20)).user_agent("Mozilla/5.0 AgentDeck").build() {
        Ok(client) => client,
        Err(error) => return failed(error.to_string()),
    };
    let page = match client.get("https://html.duckduckgo.com/html/").query(&[("q", query)]).send().await {
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
    let snippets: Vec<String> = snippet.captures_iter(&page).map(|found| clean(&found[1])).collect();
    let mut rows = Vec::new();
    for (index, found) in link.captures_iter(&page).take(8).enumerate() {
        let href = found[1].replace("&amp;", "&");
        let href = if href.starts_with("//") { format!("https:{href}") } else { href };
        // The result address is wrapped in a redirect: `?uddg=<the real address>`.
        let target = tauri::Url::parse(&href).ok().and_then(|url| url.query_pairs().find(|(key, _)| key == "uddg").map(|(_, value)| value.into_owned())).unwrap_or(href);
        rows.push(format!("{}. {}\n   {target}\n   {}", index + 1, clean(&found[2]), snippets.get(index).cloned().unwrap_or_default()));
    }
    ok(if rows.is_empty() { "no results".to_string() } else { rows.join("\n") })
}

/// The file change a Write or Edit makes. A Write over an existing file shows what it replaces, so the diff is
/// the real one and not "every line added".
fn change_for(name: &str, args: &Value, cwd: Option<&Path>) -> Option<ag_adapters::agent::FileChange> {
    let mut change = file_change(name, args)?;
    if name == "Write" {
        if let Ok(old) = std::fs::read_to_string(resolve(cwd, args["file_path"].as_str().unwrap_or_default())) {
            change.removed = old.lines().count();
            change.before = old.chars().take(3000).collect();
        }
    }
    Some(change)
}

fn read_file(args: &Value, cwd: Option<&Path>) -> Outcome {
    let Some(path) = args["file_path"].as_str() else { return failed("file_path is required") };
    let text = match std::fs::read(resolve(cwd, path)) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) => return failed(format!("could not read {path}: {error}")),
    };
    let first = args["offset"].as_u64().unwrap_or(1).max(1) as usize;
    let limit = args["limit"].as_u64().unwrap_or(2000) as usize;
    let numbered: Vec<String> = text.lines().enumerate().skip(first - 1).take(limit).map(|(index, line)| format!("{:>6}\t{line}", index + 1)).collect();
    ok(if numbered.is_empty() { "(empty)".to_string() } else { tail(&numbered.join("\n"), OUTPUT_MAX * 3) })
}

fn write_file(args: &Value, cwd: Option<&Path>) -> Outcome {
    let (Some(path), Some(content)) = (args["file_path"].as_str(), args["content"].as_str()) else { return failed("file_path and content are required") };
    let target = resolve(cwd, path);
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let existed = target.exists();
    match std::fs::write(&target, content) {
        Ok(()) => ok(format!("{} {} lines {} {path}", if existed { "Updated" } else { "Created" }, content.lines().count(), if existed { "in" } else { "in new file" })),
        Err(error) => failed(format!("could not write {path}: {error}")),
    }
}

fn edit_file(args: &Value, cwd: Option<&Path>) -> Outcome {
    let (Some(path), Some(old), Some(new)) = (args["file_path"].as_str(), args["old_string"].as_str(), args["new_string"].as_str()) else {
        return failed("file_path, old_string and new_string are required");
    };
    let target = resolve(cwd, path);
    let text = match std::fs::read_to_string(&target) {
        Ok(text) => text,
        Err(error) => return failed(format!("could not read {path}: {error}")),
    };
    match text.matches(old).count() {
        0 => failed("old_string was not found in the file"),
        1 => match std::fs::write(&target, text.replacen(old, new, 1)) {
            Ok(()) => ok(format!("Edited {path}")),
            Err(error) => failed(format!("could not write {path}: {error}")),
        },
        count if args["replace_all"].as_bool() == Some(true) => match std::fs::write(&target, text.replace(old, new)) {
            Ok(()) => ok(format!("Edited {path}: {count} replacements")),
            Err(error) => failed(format!("could not write {path}: {error}")),
        },
        count => failed(format!("old_string appears {count} times: add more surrounding text, or set replace_all")),
    }
}

/// Calls one AgentDeck browser tool through the MCP server the app runs for itself.
async fn browser_call(app: &AppHandle, chat: &str, name: &str, args: &Value) -> Outcome {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }).to_string();
    let Some(answer) = crate::mcp::handle(app, &request, chat).await else { return failed("the browser did not answer") };
    let Ok(answer) = serde_json::from_str::<Value>(&answer) else { return failed("the browser answered with something unreadable") };
    let result = &answer["result"];
    let mut text = Vec::new();
    let mut image = None;
    for part in result["content"].as_array().into_iter().flatten() {
        match part["type"].as_str() {
            Some("text") => text.push(part["text"].as_str().unwrap_or_default().to_owned()),
            Some("image") => {
                image = Some(format!("data:{};base64,{}", part["mimeType"].as_str().unwrap_or("image/png"), part["data"].as_str().unwrap_or_default()));
            }
            _ => {}
        }
    }
    let mut text = text.join("\n");
    if text.is_empty() && image.is_some() {
        text = "Screenshot taken.".into();
    }
    Outcome { text: tail(&text, OUTPUT_MAX), is_error: result["isError"].as_bool().unwrap_or(false), image }
}

enum Decision {
    Allow,
    Deny(&'static str),
    Ask,
}

/// Whether a tool call may run now, under the permission mode and what the user approved before.
/// `external`: the tool comes from an MCP server the user set up (AgentDeck's own browser tools are not external).
fn decide(turn: &Turn, name: &str, args: &Value, external: bool) -> Decision {
    let mode = turn.permission_mode.as_deref();
    let approved = turn.allowed.iter().any(|entry| entry == name || *entry == allow_entry(name, args));
    match name {
        "Read" | "Glob" | "Grep" | "WebFetch" | "WebSearch" | "Skill" | "LS" => Decision::Allow,
        _ if external => match mode {
            Some("plan") => Decision::Deny("plan mode: external tools may not run."),
            Some("auto" | "bypassPermissions") => Decision::Allow,
            _ if approved => Decision::Allow,
            _ => Decision::Ask,
        },
        "Write" | "Edit" | "MkDir" | "Move" | "Copy" => match mode {
            Some("plan") => Decision::Deny("plan mode: nothing may be changed. Describe the plan instead."),
            Some("acceptEdits" | "auto" | "bypassPermissions") => Decision::Allow,
            _ if approved => Decision::Allow,
            _ => Decision::Ask,
        },
        "Bash" | "Delete" => match mode {
            Some("plan") => Decision::Deny("plan mode: commands may not run. Describe the plan instead."),
            Some("auto" | "bypassPermissions") => Decision::Allow,
            _ if approved => Decision::Allow,
            _ => Decision::Ask,
        },
        // The browser tools of AgentDeck itself.
        _ => Decision::Allow,
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
    input_tokens: u64,
    output_tokens: u64,
}

/// Reads one `data:` line of the stream: emits text and reasoning, collects the tool calls.
fn read_chunk(data: &str, step: &mut Step, emit: &mut impl FnMut(AgentEvent)) {
    let Ok(json) = serde_json::from_str::<Value>(data) else { return };
    let delta = &json["choices"][0]["delta"];
    for key in ["reasoning_content", "reasoning"] {
        if let Some(part) = delta[key].as_str().filter(|part| !part.is_empty()) {
            emit(AgentEvent::Thinking { text: part.to_owned() });
            break;
        }
    }
    if let Some(part) = delta["content"].as_str().filter(|part| !part.is_empty()) {
        step.text.push_str(part);
        emit(AgentEvent::Delta { text: part.to_owned() });
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
        step.input_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0);
        step.output_tokens = usage["completion_tokens"].as_u64().unwrap_or(0);
    }
}

/// Sends the conversation and streams the answer. `Err` is a message for the user.
async fn model_call(client: &reqwest::Client, turn: &Turn, messages: &[Value], tools: &[Value], emit: &mut impl FnMut(AgentEvent)) -> Result<Step, String> {
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
    let mut request = client.post(chat_url(&turn.kind, &turn.base_url)).json(&body);
    if let Some(key) = turn.api_key.as_deref().filter(|key| !key.is_empty()) {
        request = request.bearer_auth(key);
    }
    let mut response = match request.send().await {
        Ok(response) => response,
        Err(error) if error.is_connect() => return Err("could not connect: is the server running?".into()),
        Err(error) => return Err(error.to_string()),
    };
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<Value>(&detail).ok().and_then(|json| json["error"]["message"].as_str().map(str::to_owned)).unwrap_or(detail);
        let detail = detail.trim();
        return Err(if detail.is_empty() { format!("the server answered {status}") } else { format!("{status}: {detail}") });
    }
    let mut step = Step::default();
    let mut pending = String::new();
    'stream: loop {
        // A server that stops sending (a stuck model, a dropped connection) must not leave the chat waiting for ever.
        let next = match tokio::time::timeout(STALL, response.chunk()).await {
            Ok(next) => next,
            Err(_) => return Err(format!("the model sent nothing for {} seconds and was stopped", STALL.as_secs())),
        };
        match next {
            Ok(Some(bytes)) => {
                pending.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(end) = pending.find('\n') {
                    let line: String = pending.drain(..=end).collect();
                    let Some(data) = line.trim().strip_prefix("data:") else { continue };
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
    let folder = cwd.map(|dir| format!(" The working folder is {}.", dir.display())).unwrap_or_default();
    let mut prompt = format!(
        "You are a coding agent running inside AgentDeck, a desktop app.{folder} You work through tools, and you only know what they show you. \
         Files: LS (list a folder), Glob (find by name), Grep (search contents), Read, Write, Edit, MkDir, Move (also renames), Copy, Delete. Shell: Bash (any command: git, build, tests, installs). Web: WebSearch, then WebFetch to read a page. \
         Look before you change: find and read the files first, then make the change, then check it (run the build or the tests). Call tools for real: never describe a tool call in text or pretend it ran. \
         Browser: the browser_* tools drive the Preview panel, the page the user is looking at. To run a dev server use browser_serve (a server started with Bash dies when the command ends), then browser_open if needed. \
         To act on a page, call browser_elements for the list of buttons, links and fields with their numbers, then browser_click_element with a number (or the visible text, or a CSS selector); use browser_fill to fill a field by selector. Do not guess coordinates. \
         A dialog, drawer or menu that is open blocks the page behind it: close it first (its close button, or browser_key Escape) before acting on anything else. \
         After each action call browser_elements or browser_dom again to see the result, and check browser_console for errors. Answer in the user's language."
    );
    if !skills.is_empty() {
        prompt.push_str("\n\nSkills you can load with the Skill tool when the task matches (name: what it is for):");
        for skill in skills.iter().take(60) {
            prompt.push_str(&format!("\n- {}: {}", skill.name, skill.description.chars().take(110).collect::<String>()));
        }
    }
    prompt
}

/// The skills and commands the user has for Claude, Codex and Antigravity (each name once), for the Skill tool.
fn skills_for(cwd: Option<&Path>) -> Vec<ProviderCommand> {
    let Ok(home) = crate::home_folder() else { return Vec::new() };
    let project = cwd.filter(|dir| dir.is_dir());
    let mut seen = std::collections::HashSet::new();
    ["claude", "codex", "antigravity"].iter().flat_map(|agent| slash_commands::list(agent, &home, project)).filter(|item| seen.insert(item.name.clone())).collect()
}

/// A tool name an OpenAI function may carry: letters, digits, `_` and `-`, at most 64 characters.
fn tool_name(server: &str, tool: &str) -> String {
    let clean = |text: &str| text.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect::<String>();
    format!("mcp__{}__{}", clean(server), clean(tool)).chars().take(64).collect()
}

/// Starts the user's MCP servers at the same time; one that fails or is slow is left out.
async fn start_servers(cwd: Option<&Path>) -> Vec<Server> {
    let project = cwd.map(Path::to_path_buf);
    let raws = tauri::async_runtime::spawn_blocking(move || crate::mcp_manage::servers_for(project.as_deref())).await.unwrap_or_default();
    let handles: Vec<_> = raws.into_iter().map(|raw| tokio::spawn(async move { tokio::time::timeout(Duration::from_secs(30), Server::start(&raw)).await })).collect();
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
pub async fn run_turn(app: &AppHandle, turn: Turn, history: Vec<ChatMessage>, mut emit: impl FnMut(AgentEvent)) {
    if !(turn.base_url.starts_with("http://") || turn.base_url.starts_with("https://")) {
        return emit(AgentEvent::Error { message: "the provider address must start with http:// or https://".into() });
    }
    if turn.kind == "anthropic" {
        return emit(AgentEvent::Error { message: "Anthropic-compatible providers cannot chat yet".into() });
    }
    let client = match reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build() {
        Ok(client) => client,
        Err(error) => return emit(AgentEvent::Error { message: error.to_string() }),
    };
    let chat: String = turn.chat.chars().filter(|c| c.is_ascii_alphanumeric() || "-_:".contains(*c)).collect();
    let mut tools = builtin_tools();
    if let Some(folder) = &turn.cwd {
        crate::index::set_root(&chat, &folder.to_string_lossy());
    }
    tools.extend(browser_tools(app, &chat).await);
    let skills = skills_for(turn.cwd.as_deref());
    // The user's own MCP servers: each tool is offered as `mcp__<server>__<tool>`, as Claude names them.
    let mut servers = start_servers(turn.cwd.as_deref()).await;
    let mut external: Vec<(String, usize, String)> = Vec::new();
    for (index, server) in servers.iter().enumerate() {
        for tool in &server.tools {
            let Some(name) = tool["name"].as_str() else { continue };
            let exposed = tool_name(&server.name, name);
            let properties = if tool["inputSchema"]["properties"].is_object() { tool["inputSchema"]["properties"].clone() } else { json!({}) };
            let required: Vec<&str> = tool["inputSchema"]["required"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            tools.push(function(&exposed, tool["description"].as_str().unwrap_or_default(), properties, &required));
            external.push((exposed, index, name.to_owned()));
        }
    }

    let mut messages: Vec<Value> = vec![json!({ "role": "system", "content": system_prompt(turn.cwd.as_deref(), &skills) })];
    messages.extend(history.iter().map(|m| json!({ "role": m.role, "content": m.content })));

    let mut answer = String::new();
    let mut retries = 0;
    for _ in 0..MAX_STEPS {
        let step = match model_call(&client, &turn, &messages, &tools, &mut emit).await {
            Ok(step) => step,
            Err(message) => return emit(AgentEvent::Error { message }),
        };
        emit(AgentEvent::Usage { input_tokens: step.input_tokens, output_tokens: step.output_tokens, cache_read_tokens: 0, cache_write_tokens: 0, new_call: true });
        answer.push_str(&step.text);
        let calls: Vec<&Call> = step.calls.iter().filter(|call| !call.name.is_empty()).collect();
        if calls.is_empty() {
            // Some local models write the tool call as text instead of using the tool interface: ask once more.
            if (step.text.contains("<tool_call>") || step.text.contains("<function=")) && retries < 2 {
                retries += 1;
                messages.push(json!({ "role": "assistant", "content": step.text }));
                messages.push(json!({ "role": "user", "content": "That tool call was written as text, so it did not run. Call the tool through the tool-calling interface." }));
                continue;
            }
            if step.finish == "length" {
                return emit(AgentEvent::Error { message: "the model hit its output limit before it finished: raise the limit or the context in the server, or ask for a smaller step".into() });
            }
            if answer.trim().is_empty() {
                return emit(AgentEvent::Error { message: "the model gave no answer (it only thought). Try Thinking: Off or Low in the model picker".into() });
            }
            return emit(AgentEvent::Done { session_id: None, text: answer, cost_usd: None });
        }

        let ids: Vec<String> = calls.iter().enumerate().map(|(i, call)| if call.id.is_empty() { format!("call_{}_{i}", messages.len()) } else { call.id.clone() }).collect();
        messages.push(json!({
            "role": "assistant",
            "content": if step.text.is_empty() { Value::Null } else { Value::String(step.text.clone()) },
            "tool_calls": calls.iter().zip(&ids).map(|(call, id)| json!({ "id": id, "type": "function", "function": { "name": call.name, "arguments": if call.arguments.is_empty() { "{}" } else { call.arguments.as_str() } } })).collect::<Vec<_>>(),
        }));

        let mut denied = Vec::new();
        for (call, id) in calls.iter().zip(&ids) {
            let args: Value = serde_json::from_str(if call.arguments.is_empty() { "{}" } else { &call.arguments }).unwrap_or(Value::Null);
            let user_tool = external.iter().find(|(exposed, _, _)| *exposed == call.name);
            let builtin = matches!(call.name.as_str(), "Bash" | "Read" | "Write" | "Edit" | "Glob" | "Grep" | "WebFetch" | "WebSearch" | "Skill" | "LS" | "MkDir" | "Move" | "Copy" | "Delete");
            let shown = if builtin || user_tool.is_some() { call.name.clone() } else { format!("{MCP_PREFIX}{}", call.name) };
            emit(AgentEvent::ToolCall { id: id.clone(), name: shown, input: summarize_tool_input(&call.name, &args), change: change_for(&call.name, &args, turn.cwd.as_deref()), background: false });
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
                        "Bash" => run_bash(args["command"].as_str().unwrap_or_default(), turn.cwd.as_deref()).await,
                        "Read" => read_file(&args, turn.cwd.as_deref()),
                        "Write" => write_file(&args, turn.cwd.as_deref()),
                        "Edit" => edit_file(&args, turn.cwd.as_deref()),
                        "Glob" | "Grep" => {
                            let (name, cwd, args) = (call.name.clone(), turn.cwd.clone(), args.clone());
                            tokio::task::spawn_blocking(move || if name == "Glob" { glob_files(&args, cwd.as_deref()) } else { grep_files(&args, cwd.as_deref()) })
                                .await
                                .unwrap_or_else(|_| failed("the search failed"))
                        }
                        "WebFetch" => web_fetch(&args).await,
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
                                    Ok(text) => ok(tail(&text, OUTPUT_MAX * 2)),
                                    Err(error) => failed(error),
                                },
                                _ => failed(format!("no skill named {wanted}")),
                            }
                        }
                        name => match user_tool {
                            Some((_, index, tool)) => {
                                let reply = servers[*index].call(tool, &args).await;
                                Outcome { text: tail(&reply.text, OUTPUT_MAX), is_error: reply.is_error, image: reply.image }
                            }
                            None => tokio::time::timeout(TOOL_LIMIT, browser_call(app, &chat, name, &args)).await.unwrap_or_else(|_| failed("the browser did not answer in time. Try browser_open again, then browser_elements.")),
                        },
                    },
                }
            };
            emit(AgentEvent::ToolResult { id: id.clone(), output: outcome.text.clone(), is_error: outcome.is_error, image: outcome.image.clone() });
            messages.push(json!({ "role": "tool", "tool_call_id": id, "content": outcome.text }));
        }
        if !denied.is_empty() {
            emit(AgentEvent::PermissionDenied { requests: denied });
            return emit(AgentEvent::Done { session_id: None, text: answer, cost_usd: None });
        }
    }
    emit(AgentEvent::Error { message: format!("stopped after {MAX_STEPS} steps without a final answer") });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(mode: Option<&str>, allowed: &[&str]) -> Turn {
        Turn { kind: "openai".into(), base_url: String::new(), api_key: None, model: String::new(), cwd: None, permission_mode: mode.map(str::to_owned), allowed: allowed.iter().map(|s| s.to_string()).collect(), chat: String::new(), effort: None }
    }

    #[test]
    fn chat_route_follows_the_kind() {
        assert_eq!(chat_url("openai", "http://h:1234/v1/"), "http://h:1234/v1/chat/completions");
        assert_eq!(chat_url("ollama", "http://h:11434"), "http://h:11434/v1/chat/completions");
    }

    #[test]
    fn chunks_give_text_reasoning_tool_calls_and_usage() {
        let mut step = Step::default();
        let mut events = Vec::new();
        let mut emit = |event| events.push(event);
        read_chunk(r#"{"choices":[{"delta":{"reasoning_content":"hm","content":"hi"}}]}"#, &mut step, &mut emit);
        read_chunk(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"Bash","arguments":"{\"comm"}}]}}]}"#, &mut step, &mut emit);
        read_chunk(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"and\":\"ls\"}"}}]}}]}"#, &mut step, &mut emit);
        read_chunk(r#"{"choices":[],"usage":{"prompt_tokens":3,"completion_tokens":5}}"#, &mut step, &mut emit);
        read_chunk("not json", &mut step, &mut emit);
        assert_eq!(events, [AgentEvent::Thinking { text: "hm".into() }, AgentEvent::Delta { text: "hi".into() }]);
        assert_eq!((step.text.as_str(), step.input_tokens, step.output_tokens), ("hi", 3, 5));
        assert_eq!((step.calls[0].id.as_str(), step.calls[0].name.as_str(), step.calls[0].arguments.as_str()), ("c1", "Bash", r#"{"command":"ls"}"#));
    }

    #[test]
    fn permissions_follow_the_mode() {
        let write = json!({ "file_path": "a.txt", "content": "x" });
        let bash = json!({ "command": "git status" });
        assert!(matches!(decide(&turn(None, &[]), "Read", &Value::Null, false), Decision::Allow));
        assert!(matches!(decide(&turn(None, &[]), "Write", &write, false), Decision::Ask));
        assert!(matches!(decide(&turn(Some("acceptEdits"), &[]), "Write", &write, false), Decision::Allow));
        assert!(matches!(decide(&turn(Some("acceptEdits"), &[]), "Bash", &bash, false), Decision::Ask));
        assert!(matches!(decide(&turn(Some("auto"), &[]), "Bash", &bash, false), Decision::Allow));
        assert!(matches!(decide(&turn(Some("plan"), &[]), "Bash", &bash, false), Decision::Deny(_)));
        assert!(matches!(decide(&turn(None, &["Bash(git status)"]), "Bash", &bash, false), Decision::Allow));
        assert!(matches!(decide(&turn(None, &["Write"]), "Write", &write, false), Decision::Allow));
    }

    #[test]
    fn files_can_be_made_found_moved_and_deleted_inside_the_project() {
        let dir = std::env::temp_dir().join(format!("agentdeck-tools-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cwd = Some(dir.as_path());
        assert!(!make_dir(&json!({ "path": "src/lib" }), cwd).is_error);
        assert!(!write_file(&json!({ "file_path": "src/lib/a.ts", "content": "let needle = 1;\n" }), cwd).is_error);
        assert_eq!(glob_files(&json!({ "pattern": "*.ts" }), cwd).text, "src/lib/a.ts");
        assert!(grep_files(&json!({ "pattern": "needle" }), cwd).text.starts_with("src/lib/a.ts:1:"));
        assert!(list_dir(&json!({ "path": "src" }), cwd).text.contains("lib/"));
        assert!(!transfer(&json!({ "from": "src/lib/a.ts", "to": "src/b.ts" }), cwd, true).is_error);
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
        let edit = |old: &str| edit_file(&json!({ "file_path": "f.txt", "old_string": old, "new_string": "z" }), Some(&dir));
        assert!(edit("a").is_error);
        assert!(edit("q").is_error);
        assert!(!edit("b").is_error);
        assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "a z a");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
