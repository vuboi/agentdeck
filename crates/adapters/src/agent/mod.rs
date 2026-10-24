//! Agent CLIs behind one event stream.
//!
//! Each provider knows its own flags and its own output format. The rest of the app only sees
//! [`AgentEvent`], so adding a CLI means adding one [`Provider`] impl and nothing else.

pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod runner;

use serde::Serialize;

/// One thing an agent did or said, in the shape the chat UI renders.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Visible answer text, as it streams.
    Delta { text: String },
    /// Reasoning text, as it streams.
    Thinking { text: String },
    /// A tool the agent started. `id` links it to its [`AgentEvent::ToolResult`]; `input` is a one-line
    /// summary of what it was asked to do (the command, the file, the pattern).
    ToolCall {
        id: String,
        name: String,
        input: String,
        /// Set when the tool writes a file: what it adds and removes.
        change: Option<FileChange>,
        /// Runs on its own while the agent carries on: a shell started in the background, or a sub-agent.
        background: bool,
    },
    /// What a tool printed. `is_error` marks a failed call.
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
        /// A picture the tool returned (a browser screenshot), as a `data:` URL, shown as a thumbnail.
        #[serde(skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    /// The CLI refused tools that need the user's approval. Allow them and send a follow-up turn to continue.
    PermissionDenied { requests: Vec<PermissionRequest> },
    /// The turn finished. `session_id` is the CLI's id for the conversation, to resume next turn.
    Done {
        session_id: Option<String>,
        text: String,
        cost_usd: Option<f64>,
    },
    /// Tokens of the model call in progress. A new call (`new_call`) starts at its input; then its output
    /// count grows (each report is the call's total so far, not an increment).
    Usage {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
        new_call: bool,
    },
    /// The turn failed.
    Error { message: String },
    /// A line of the CLI's output that the app does not understand (a new kind of step). Not shown in the chat;
    /// kept in the Audit log as written, so it can be copied to teach the adapter about it.
    Raw { source: String, label: String, line: String },
}

/// The address of the browser's MCP server for one chat: `/mcp?chat=<id>&root=<folder>`. The bridge reads both from it.
pub fn mcp_url(bridge: &str, owner: Option<&str>, cwd: Option<&std::path::Path>) -> String {
    let mut query = Vec::new();
    if let Some(chat) = owner {
        let safe: String = chat.chars().filter(|c| c.is_ascii_alphanumeric() || "-_:".contains(*c)).collect();
        query.push(format!("chat={safe}"));
    }
    // The project folder, for the code index.
    if let Some(folder) = cwd {
        let encoded: String = folder
            .to_string_lossy()
            .bytes()
            .map(|byte| if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) { (byte as char).to_string() } else { format!("%{byte:02X}") })
            .collect();
        query.push(format!("root={encoded}"));
    }
    if query.is_empty() {
        format!("{bridge}/mcp")
    } else {
        format!("{bridge}/mcp?{}", query.join("&"))
    }
}

/// One user message to send to an agent.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnRequest {
    pub prompt: String,
    /// The CLI's conversation id. `None` starts a new conversation.
    pub session_id: Option<String>,
    /// `true` continues `session_id`; `false` creates a conversation with that id.
    pub resume: bool,
    pub model: Option<String>,
    /// The CLI's permission mode (`acceptEdits`, `plan`, `auto`, `bypassPermissions`); `None` asks for every tool.
    pub permission_mode: Option<String>,
    /// Reasoning effort (`low`, `medium`, `high`, `max`); `None` leaves the CLI's default.
    pub effort: Option<String>,
    /// Tools approved by the user, as the CLI writes them: `Bash(git status)`, `Write`.
    pub allowed_tools: Vec<String>,
    /// Folder the CLI runs in. Claude files a conversation under the folder it ran from, so this is the project.
    pub cwd: Option<std::path::PathBuf>,
    /// The AgentDeck chat this turn belongs to: the browser tools it calls are tagged with it (which chat drives a tab).
    pub owner: Option<String>,
}

/// Everything provider-specific: how to start a turn and how to read its output.
pub trait Provider: Send + Sync {
    /// Short name, used in events and logs.
    fn id(&self) -> &'static str;
    /// Executable to run, looked up on PATH unless `candidates` says otherwise.
    fn binary(&self) -> &'static str;
    /// Extra places to look for the binary, tried before PATH (for apps started without a shell PATH).
    fn candidates(&self) -> Vec<std::path::PathBuf>;
    /// Command-line arguments for one turn.
    fn args(&self, req: &TurnRequest) -> Vec<String>;
    /// One line of stdout to zero or more events. Lines this provider does not understand give none.
    fn parse_line(&self, line: &str) -> Vec<AgentEvent>;
    /// Runs before the CLI starts, for what a turn needs that flags cannot say (a rule in the CLI's own settings).
    /// An error stops the turn and is shown as its error.
    fn prepare(&self, _req: &TurnRequest) -> Result<(), String> {
        Ok(())
    }
}

/// A plan tool's list (Claude `TodoWrite` `todos`, Codex `update_plan` `plan`) as `[{"content","status"}]` JSON, whole:
/// the chat draws it as a checklist, so it must not be cut like other inputs.
pub fn plan_json(input: &serde_json::Value) -> Option<String> {
    let list = input.get("todos").or_else(|| input.get("plan"))?.as_array()?;
    let items: Vec<serde_json::Value> = list
        .iter()
        .filter_map(|item| {
            let content = item.get("content").or_else(|| item.get("step"))?.as_str()?;
            let status = item.get("status").and_then(serde_json::Value::as_str).unwrap_or("pending");
            Some(serde_json::json!({ "content": content, "status": status }))
        })
        .collect();
    Some(serde_json::Value::Array(items).to_string())
}

/// One line describing a tool call's input: the part a person would look at.
pub fn summarize_tool_input(name: &str, input: &serde_json::Value) -> String {
    if matches!(name, "TodoWrite" | "update_plan") {
        if let Some(plan) = plan_json(input) {
            return plan;
        }
    }
    let field = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| input.get(*key)?.as_str().map(str::to_owned))
    };
    let picked = match name {
        "Bash" => field(&["command"]),
        "Read" | "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => {
            field(&["file_path", "notebook_path"])
        }
        "Grep" | "Glob" => field(&["pattern"]),
        "WebFetch" => field(&["url"]),
        "WebSearch" => field(&["query"]),
        "Task" | "Agent" => field(&["description", "prompt"]),
        _ => None,
    };
    let text = picked.unwrap_or_else(|| {
        let compact = input.to_string();
        if compact == "null" || compact == "{}" {
            String::new()
        } else {
            compact
        }
    });
    if text.chars().count() > 400 {
        let mut cut: String = text.chars().take(400).collect();
        cut.push('…');
        cut
    } else {
        text
    }
}

/// What one Write or Edit does to a file, for the "created 2 files +222 -0" summary and its diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileChange {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    /// Text that was replaced (empty for a new file), cut to a few thousand characters.
    pub before: String,
    /// Text that took its place, cut the same way.
    pub after: String,
}

const DIFF_TEXT_MAX: usize = 3000;

fn line_count(text: &str) -> usize {
    text.lines().count()
}

fn cut(text: &str) -> String {
    if text.chars().count() > DIFF_TEXT_MAX {
        let mut shortened: String = text.chars().take(DIFF_TEXT_MAX).collect();
        shortened.push('…');
        shortened
    } else {
        text.to_owned()
    }
}

/// The first picture in a tool result's content, as a `data:` URL. Claude nests it under `source`; an MCP
/// result (Codex) has `data` and `mimeType` on the part itself.
pub fn result_image(content: &serde_json::Value) -> Option<String> {
    let part = content.as_array()?.iter().find(|part| part["type"] == "image")?;
    let (mime, data) = match part.get("source") {
        Some(source) => (source["media_type"].as_str()?, source["data"].as_str()?),
        None => (part["mimeType"].as_str()?, part["data"].as_str()?),
    };
    Some(format!("data:{mime};base64,{data}"))
}

/// The file change a tool call makes, if it is one of the file-writing tools.
pub fn file_change(name: &str, input: &serde_json::Value) -> Option<FileChange> {
    let text =
        |value: &serde_json::Value, key: &str| value[key].as_str().unwrap_or_default().to_owned();
    let path = ["file_path", "notebook_path"]
        .iter()
        .find_map(|key| input[*key].as_str())?
        .to_owned();
    match name {
        "Write" => {
            let content = text(input, "content");
            Some(FileChange {
                path,
                added: line_count(&content),
                removed: 0,
                before: String::new(),
                after: cut(&content),
            })
        }
        "Edit" => {
            let (old, new) = (text(input, "old_string"), text(input, "new_string"));
            Some(FileChange {
                path,
                added: line_count(&new),
                removed: line_count(&old),
                before: cut(&old),
                after: cut(&new),
            })
        }
        "MultiEdit" => {
            let edits = input["edits"].as_array()?;
            let (mut before, mut after) = (Vec::new(), Vec::new());
            for edit in edits {
                before.push(text(edit, "old_string"));
                after.push(text(edit, "new_string"));
            }
            Some(FileChange {
                path,
                added: after.iter().map(|part| line_count(part)).sum(),
                removed: before.iter().map(|part| line_count(part)).sum(),
                before: cut(&before.join("\n…\n")),
                after: cut(&after.join("\n…\n")),
            })
        }
        _ => None,
    }
}

/// A tool call the CLI refused, with what to pass to `--allowedTools` to let it through.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PermissionRequest {
    pub id: String,
    pub tool: String,
    /// One line: the command, the file.
    pub summary: String,
    /// The `--allowedTools` entry that approves exactly this.
    pub allow: String,
}

/// `Bash(<command>)` for shell commands, the bare tool name for everything else.
pub fn allow_entry(tool: &str, input: &serde_json::Value) -> String {
    match (tool, input["command"].as_str()) {
        ("Bash", Some(command)) => format!("Bash({command})"),
        _ => tool.to_owned(),
    }
}
