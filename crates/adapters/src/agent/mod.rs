//! Agent CLIs behind one event stream.
//!
//! Each provider knows its own flags and its own output format. The rest of the app only sees
//! [`AgentEvent`], so adding a CLI means adding one [`Provider`] impl and nothing else.

pub mod antigravity;
pub mod claude;
pub mod codex;
mod codex_server;
pub mod interrupt;
pub mod opencode;
pub mod permission;
pub mod resolve;
pub mod runner;
pub mod stall;
pub mod warm;

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
        /// A file diff captured after a write tool completed when its start event only had a path.
        #[serde(skip_serializing_if = "Option::is_none")]
        change: Option<FileChange>,
    },
    /// New command output written since the last update, correlated to the running tool call.
    ToolOutputDelta { id: String, delta: String },
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
    Raw {
        source: String,
        label: String,
        line: String,
    },
}

/// The address of the browser's MCP server for one chat: `/mcp?chat=<id>&root=<folder>`. The bridge reads both from it.
pub fn mcp_url(bridge: &str, owner: Option<&str>, cwd: Option<&std::path::Path>) -> String {
    let mut query = Vec::new();
    if let Some(chat) = owner {
        let safe: String = chat
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || "-_:".contains(*c))
            .collect();
        query.push(format!("chat={safe}"));
    }
    // The project folder, for the code index.
    if let Some(folder) = cwd {
        let encoded: String = folder
            .to_string_lossy()
            .bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
                    (byte as char).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
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
    /// An Anthropic API key: Claude Code then bills the Console's API credit instead of the signed-in plan.
    pub api_key: Option<String>,
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
    /// `true` when `args` leaves the prompt out and the CLI reads it from stdin. Windows caps a command line at
    /// about 32k characters, so a long prompt (pasted files, logs) has to go this way.
    fn prompt_on_stdin(&self) -> bool {
        false
    }
    /// One line of stdout to zero or more events. Lines this provider does not understand give none.
    fn parse_line(&self, line: &str) -> Vec<AgentEvent>;
    /// Runs before the CLI starts, for what a turn needs that flags cannot say (a rule in the CLI's own settings).
    /// An error stops the turn and is shown as its error.
    fn prepare(&self, _req: &TurnRequest) -> Result<(), String> {
        Ok(())
    }
    /// Environment additions for CLIs whose configuration is supplied per turn (for example MCP).
    fn environment(&self, _req: &TurnRequest) -> Vec<(String, String)> {
        Vec::new()
    }
    /// How long a turn may print nothing before it is ended as stuck (see `stall.rs`). Any output line resets it.
    fn stall_limit(&self) -> std::time::Duration {
        stall::DEFAULT_STALL_LIMIT
    }
    /// Emits a terminal event when a CLI exits without one in its stream.
    fn finish(&self, _req: &TurnRequest) -> Option<AgentEvent> {
        None
    }
    /// Arguments for a CLI that stays open between the turns of a chat and reads each message as one JSON line on
    /// stdin (see `warm.rs`). `None`: one process per turn. Starting a CLI loads its settings, plugins, skills, MCP
    /// servers and hooks every time; a kept process pays that once.
    fn warm_args(&self, _req: &TurnRequest) -> Option<Vec<String>> {
        None
    }
    /// The JSON line (without its newline) that sends one message to a warm process.
    fn warm_message(&self, _prompt: &str) -> String {
        String::new()
    }
    /// How a stop reaches this CLI while it runs a turn on a kept process (see `interrupt.rs`). The default kills it.
    fn interrupt_mode(&self) -> interrupt::InterruptMode {
        interrupt::InterruptMode::Kill
    }
    /// Whether a kept process can switch its model in the middle of a conversation (`model_switch_line`), so a model
    /// change keeps the process and its conversation. Only then is the model left out of the process's settings key.
    fn live_model(&self) -> bool {
        false
    }
    /// The line (without its newline) that switches a kept process to `model`; only used when `live_model` is true.
    fn model_switch_line(&self, _request_id: &str, _model: &str) -> String {
        String::new()
    }
}

/// A plan tool's list (Claude `TodoWrite` `todos`, Codex `update_plan` `plan`) as `[{"content","status"}]` JSON, whole:
/// the chat draws it as a checklist, so it must not be cut like other inputs.
pub fn plan_json(input: &serde_json::Value) -> Option<String> {
    let list = input
        .get("todos")
        .or_else(|| input.get("plan"))?
        .as_array()?;
    let items: Vec<serde_json::Value> = list
        .iter()
        .filter_map(|item| {
            let content = item.get("content").or_else(|| item.get("step"))?.as_str()?;
            let status = item
                .get("status")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("pending");
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

/// A bounded before-image for a file-writing tool whose provider only reports a path.
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    path: String,
    full_path: std::path::PathBuf,
    before: Option<String>,
}

/// Capture a file before a write tool, constrained to the active project root.
pub fn begin_file_snapshot(
    root: Option<&std::path::Path>,
    name: &str,
    input: &serde_json::Value,
) -> Option<FileSnapshot> {
    let root = root?.canonicalize().ok()?;
    if !is_write_tool(name) {
        return None;
    }
    let raw_path = ["file_path", "notebook_path", "filePath", "notebookPath", "Path", "AbsolutePath", "absolute_path", "TargetFile", "target_file", "path"]
        .iter()
        .find_map(|key| input[*key].as_str())?;
    let supplied = std::path::Path::new(raw_path);
    let candidate = if supplied.is_absolute() { supplied.to_path_buf() } else { root.join(supplied) };
    let (full_path, before) = match candidate.canonicalize() {
        Ok(path) if path.starts_with(&root) => {
            let text = read_change_text(&path)?;
            (path, Some(text))
        }
        Ok(_) => return None,
        Err(_) => {
            let parent = candidate.parent()?.canonicalize().ok()?;
            let full_path = parent.join(candidate.file_name()?);
            if !full_path.starts_with(&root) { return None; }
            (full_path, None)
        }
    };
    Some(FileSnapshot { path: raw_path.to_owned(), full_path, before })
}

/// Finish a before-image after a successful write and return only the changed lines.
pub fn finish_file_snapshot(snapshot: FileSnapshot) -> Option<FileChange> {
    let after = if snapshot.full_path.exists() { read_change_text(&snapshot.full_path)? } else { String::new() };
    let before = snapshot.before.as_deref().unwrap_or_default();
    let diff = line_diff(before, &after);
    if diff.added == 0 && diff.removed == 0 { return None; }
    Some(FileChange { path: snapshot.path, added: diff.added, removed: diff.removed, before: diff.before, after: diff.after })
}

fn read_change_text(path: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > 2_000_000 || bytes.contains(&0) { return None; }
    String::from_utf8(bytes).ok()
}

fn is_write_tool(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "write" | "edit" | "multiedit" | "notebookedit" | "apply_patch" | "patch" | "write_file" | "writefile" | "create_file" | "createfile" | "edit_file" | "editfile" | "replace_file_content" | "replacefilecontent")
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

/// Past this many cells the exact comparison is skipped: the middle part is then counted as all changed.
const LCS_CELLS_MAX: usize = 25_000_000;

/// The lines a file change adds and removes, counted the way a diff counts them: a line that stays is neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineDiff {
    pub added: usize,
    pub removed: usize,
    /// The changed middle of the old text, cut like the other diff texts.
    pub before: String,
    /// The changed middle of the new text.
    pub after: String,
}

/// Length of the longest common run of lines (the lines a diff keeps).
fn kept_lines(old: &[&str], new: &[&str]) -> usize {
    if old.is_empty() || new.is_empty() || old.len().saturating_mul(new.len()) > LCS_CELLS_MAX {
        return 0;
    }
    let mut previous = vec![0usize; new.len() + 1];
    let mut current = vec![0usize; new.len() + 1];
    for line in old {
        for (index, other) in new.iter().enumerate() {
            current[index + 1] = if line == other {
                previous[index] + 1
            } else {
                previous[index + 1].max(current[index])
            };
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[new.len()]
}

/// Compares the whole text before and after a change. Only the lines between the first and last difference are
/// compared, so a one-line edit in a long file counts as one line, not as the whole file.
pub fn line_diff(before: &str, after: &str) -> LineDiff {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_middle = &old[prefix..old.len() - suffix];
    let new_middle = &new[prefix..new.len() - suffix];
    let kept = kept_lines(old_middle, new_middle);
    LineDiff {
        added: new_middle.len() - kept,
        removed: old_middle.len() - kept,
        before: cut(&old_middle.join("\n")),
        after: cut(&new_middle.join("\n")),
    }
}

#[cfg(test)]
mod line_diff_tests {
    use super::line_diff;

    #[test]
    fn a_one_line_edit_in_a_long_file_counts_one_line() {
        let before: String = (0..400).map(|n| format!("line {n}\n")).collect();
        let after = before.replace("line 200\n", "changed\n");
        let diff = line_diff(&before, &after);
        assert_eq!((diff.added, diff.removed), (1, 1));
        assert_eq!(diff.before, "line 200");
        assert_eq!(diff.after, "changed");
    }

    #[test]
    fn scattered_edits_count_only_the_changed_lines() {
        let diff = line_diff("a\nb\nc\nd\ne\n", "a\nB\nc\nD\ne\n");
        assert_eq!((diff.added, diff.removed), (2, 2));
    }

    #[test]
    fn a_new_file_counts_every_line_as_added() {
        let diff = line_diff("", "x\ny\n");
        assert_eq!((diff.added, diff.removed), (2, 0));
    }
}

/// The first picture in a tool result's content, as a `data:` URL. Claude nests it under `source`; an MCP
/// result (Codex) has `data` and `mimeType` on the part itself.
pub fn result_image(content: &serde_json::Value) -> Option<String> {
    let part = content
        .as_array()?
        .iter()
        .find(|part| part["type"] == "image")?;
    let (mime, data) = match part.get("source") {
        Some(source) => (source["media_type"].as_str()?, source["data"].as_str()?),
        None => (part["mimeType"].as_str()?, part["data"].as_str()?),
    };
    Some(format!("data:{mime};base64,{data}"))
}

/// The file change a tool call makes, if it is one of the file-writing tools.
fn string_arg<'a>(source: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| source[*key].as_str())
}

pub fn file_change(name: &str, input: &serde_json::Value) -> Option<FileChange> {
    let text = |source: &serde_json::Value, keys: &[&str]| string_arg(source, keys).unwrap_or_default().to_owned();
    let path = ["file_path", "notebook_path", "filePath", "notebookPath", "Path", "AbsolutePath", "absolute_path", "TargetFile", "target_file", "path"]
        .iter()
        .find_map(|key| input[*key].as_str())?
        .to_owned();
    match name.to_ascii_lowercase().as_str() {
        "write" | "write_file" | "writefile" | "create_file" | "createfile" => {
            let content = string_arg(input, &["content", "Content", "text"])?.to_owned();
            Some(FileChange {
                path,
                added: line_count(&content),
                removed: 0,
                before: String::new(),
                after: cut(&content),
            })
        }
        "edit" | "edit_file" | "editfile" | "replace_file_content" | "replacefilecontent" => {
            if string_arg(input, &["old_string", "oldString", "oldText", "old_text", "oldContent"]).is_none()
                && string_arg(input, &["new_string", "newString", "newText", "new_text", "newContent", "replacement"]).is_none()
            {
                return None;
            }
            let (old, new) = (text(input, &["old_string", "oldString", "oldText", "old_text", "oldContent"]), text(input, &["new_string", "newString", "newText", "new_text", "newContent", "replacement"]));
            Some(FileChange {
                path,
                added: line_count(&new),
                removed: line_count(&old),
                before: cut(&old),
                after: cut(&new),
            })
        }
        "multiedit" | "notebookedit" => {
            let edits = input["edits"].as_array()?;
            let (mut before, mut after) = (Vec::new(), Vec::new());
            for edit in edits {
                before.push(text(edit, &["old_string", "oldString", "oldText"]));
                after.push(text(edit, &["new_string", "newString", "newText"]));
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
