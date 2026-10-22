//! Codex sessions: `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<uuid>.jsonl`.
//!
//! One file per session. The first line is `session_meta` (id, cwd). The transcript is the
//! `response_item` lines: `message`, `reasoning`, `function_call` / `custom_tool_call` and their
//! `*_output` lines. Everything else (`event_msg`, `turn_context`, `token_count`…) is skipped.

use std::{collections::HashMap, path::Path};

use serde_json::Value;

use super::util::{describe_tool, jsonl_files, read_lines, strip_ansi, truncate};
use super::{HistoryBlock, HistoryMessage, HistorySession, Source};

const OUTPUT_LIMIT: usize = 6_000;

/// Only these lines matter; the rest of a rollout is token counts and turn bookkeeping.
fn wanted(line: &str) -> bool {
    line.contains("\"type\":\"response_item\"")
        || line.contains("\"type\":\"session_meta\"")
        || line.contains("\"type\":\"event_msg\"")
}

pub(super) fn list(root: &Path, index: &Path) -> Vec<HistorySession> {
    let names = thread_names(index);
    jsonl_files(root)
        .iter()
        .filter_map(|path| index_file(path))
        .map(|mut session| {
            // A name given in Codex (or by AgentDeck's rename) wins over the first prompt.
            let id = session.id.strip_prefix("codex:").unwrap_or_default();
            if let Some(name) = names.get(id) {
                session.title = name.clone();
            }
            session
        })
        .collect()
}

/// `session_index.jsonl` is append-only; the last line for an id is its current name.
fn thread_names(index: &Path) -> HashMap<String, String> {
    let mut names = HashMap::new();
    for line in read_lines(index, |_| true).unwrap_or_default() {
        let (id, name) = (str_of(&line, "id"), str_of(&line, "thread_name"));
        if !id.is_empty() && !name.is_empty() {
            names.insert(id.to_owned(), name.to_owned());
        }
    }
    names
}

fn index_file(path: &Path) -> Option<HistorySession> {
    let lines = read_lines(path, wanted).ok()?;
    let meta = lines
        .iter()
        .find(|line| str_of(line, "type") == "session_meta")?;
    let payload = meta.get("payload")?;
    let id = str_of(payload, "id");
    if id.is_empty() {
        return None;
    }

    let (mut count, mut title) = (0usize, None);
    let mut updated = None;
    for line in &lines {
        let stamp = str_of(line, "timestamp");
        if !stamp.is_empty() {
            updated = Some(stamp.to_owned());
        }
        if let Some((role, text)) = message_of(line) {
            count += 1;
            if title.is_none() && role == "user" && !text.starts_with('<') {
                title = Some(truncate(&text, 80));
            }
        }
    }
    if count == 0 {
        return None;
    }

    let model = lines.iter().rev().find_map(|line| {
        let payload = line.get("payload")?;
        if str_of(line, "type") != "event_msg" || str_of(payload, "type") != "thread_settings_applied" {
            return None;
        }
        let settings = payload.get("thread_settings")?;
        let model = str_of(settings, "model");
        (!model.is_empty()).then(|| model.to_owned())
    });

    Some(HistorySession {
        id: format!("codex:{id}"),
        source: Source::Codex,
        title: title.unwrap_or_else(|| "Untitled session".to_owned()),
        model,
        cwd: str_of(payload, "cwd").to_owned(),
        started_at: Some(str_of(payload, "timestamp").to_owned()).filter(|stamp| !stamp.is_empty()),
        updated_at: updated,
        message_count: count,
        path: path.to_string_lossy().into_owned(),
        parent_id: None,
        fork_seq: None,
    })
}

pub(super) fn load(path: &Path) -> std::io::Result<Vec<HistoryMessage>> {
    let lines = read_lines(path, wanted)?;
    let mut messages: Vec<HistoryMessage> = Vec::new();
    // call_id -> (message index, block index)
    let mut pending: HashMap<String, (usize, usize)> = HashMap::new();

    for (index, line) in lines.iter().enumerate() {
        if str_of(line, "type") != "response_item" {
            continue;
        }
        let Some(payload) = line.get("payload") else {
            continue;
        };
        let time = str_of(line, "timestamp").to_owned();

        let (role, block) = match str_of(payload, "type") {
            "message" => match message_of(line) {
                Some((role, text)) => (role, HistoryBlock::Response { text }),
                None => continue,
            },
            "reasoning" => {
                let text = summary_text(payload);
                if text.is_empty() {
                    continue;
                }
                ("assistant".to_owned(), HistoryBlock::Thinking { text })
            }
            "function_call" | "custom_tool_call" => {
                let input = match payload.get("arguments").or_else(|| payload.get("input")) {
                    Some(Value::String(raw)) => {
                        serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.clone()))
                    }
                    Some(other) => other.clone(),
                    None => Value::Null,
                };
                // A patch (apply_patch) is raw text: its first file gives the command and the change.
                let patch = match &input {
                    // The patch rides in the input whatever tool carries it: apply_patch, or exec with the patch in a script.
                    Value::String(raw) if raw.contains("*** Begin Patch") => patch_change(raw),
                    _ => None,
                };
                let (command, change) = match (patch, &input) {
                    (Some((name, change)), _) => (format!("{name}  {}", change.path), Some(change)),
                    (None, Value::String(raw)) => (truncate(&format!("{}  {raw}", str_of(payload, "name")), 240), None),
                    (None, _) => (describe_tool(str_of(payload, "name"), &input), None),
                };
                (
                    "assistant".to_owned(),
                    HistoryBlock::Command {
                        command,
                        output: String::new(),
                        status: "ok".into(),
                        change,
                    },
                )
            }
            "function_call_output" | "custom_tool_call_output" => {
                attach_output(&mut messages, &pending, payload);
                continue;
            }
            _ => continue,
        };

        let call_id = str_of(payload, "call_id").to_owned();
        let is_call = matches!(block, HistoryBlock::Command { .. });
        let merge =
            role == "assistant" && messages.last().is_some_and(|last| last.role == "assistant");
        if merge {
            let last = messages.len() - 1;
            messages[last].blocks.push(block);
        } else {
            messages.push(HistoryMessage {
                usage: None,
                model: None,
                id: format!("codex-{index}"),
                role,
                time,
                blocks: vec![block],
            });
        }
        if is_call && !call_id.is_empty() {
            let message = messages.len() - 1;
            pending.insert(call_id, (message, messages[message].blocks.len() - 1));
        }
    }
    Ok(messages)
}

fn attach_output(
    messages: &mut [HistoryMessage],
    pending: &HashMap<String, (usize, usize)>,
    payload: &Value,
) {
    let Some(&(message, block)) = pending.get(str_of(payload, "call_id")) else {
        return;
    };
    let Some(HistoryBlock::Command { output, status, .. }) = messages
        .get_mut(message)
        .and_then(|m| m.blocks.get_mut(block))
    else {
        return;
    };
    let raw = payload.get("output").cloned().unwrap_or(Value::Null);
    // Output is either text or a JSON string like {"output": "...", "metadata": {"exit_code": 0}}.
    let (text, failed) = match &raw {
        Value::String(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(object)) => {
                let body = object
                    .get("output")
                    .and_then(Value::as_str)
                    .unwrap_or(text)
                    .to_owned();
                let code = object
                    .get("metadata")
                    .and_then(|m| m.get("exit_code"))
                    .and_then(Value::as_i64);
                (body, code.is_some_and(|code| code != 0))
            }
            _ => (text.clone(), false),
        },
        other => (other.to_string(), false),
    };
    *output = truncate(&strip_ansi(&text), OUTPUT_LIMIT);
    if failed {
        *status = "fail".into();
    }
}

/// `(role, text)` of a user or assistant `message` item, skipping injected context
/// (environment, AGENTS.md, instructions) that Codex stores as user messages.
fn message_of(line: &Value) -> Option<(String, String)> {
    let payload = line.get("payload")?;
    if str_of(line, "type") != "response_item" || str_of(payload, "type") != "message" {
        return None;
    }
    let role = str_of(payload, "role");
    if role != "user" && role != "assistant" {
        return None;
    }
    let text = payload
        .get("content")?
        .as_array()?
        .iter()
        .filter(|item| matches!(str_of(item, "type"), "input_text" | "output_text"))
        .map(|item| str_of(item, "text"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    let injected = text.starts_with("<environment_context>")
        || text.starts_with("<user_instructions>")
        || text.starts_with("# AGENTS.md")
        || text.starts_with("<permissions");
    (!text.is_empty() && !injected).then(|| (role.to_owned(), text))
}

fn summary_text(payload: &Value) -> String {
    payload
        .get("summary")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| str_of(item, "text"))
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The first file of an `apply_patch` body: the tool name the live view uses (Write, Edit or Delete) and what it did.
/// Lines are counted up to the next `***` header; a delete or an add has no old or new text to show.
fn patch_change(raw: &str) -> Option<(&'static str, crate::agent::FileChange)> {
    // A patch inside a script keeps its line breaks as the two characters `\n`: read them as breaks.
    let text = raw.replace("\\n", "\n");
    let lines: Vec<&str> = text.lines().collect();
    let (start, name, path) = lines.iter().enumerate().find_map(|(index, line)| {
        if let Some(path) = line.strip_prefix("*** Add File: ") {
            Some((index, "Write", path.trim()))
        } else if let Some(path) = line.strip_prefix("*** Update File: ") {
            Some((index, "Edit", path.trim()))
        } else {
            line.strip_prefix("*** Delete File: ").map(|path| (index, "Delete", path.trim()))
        }
    })?;
    let body: Vec<&str> = lines[start + 1..].iter().take_while(|line| !line.starts_with("***")).copied().collect();
    let added = body.iter().filter(|line| line.starts_with('+')).count();
    let removed = body.iter().filter(|line| line.starts_with('-')).count();
    Some((
        name,
        crate::agent::FileChange { path: path.to_owned(), added, removed, before: String::new(), after: String::new() },
    ))
}

#[cfg(test)]
mod patch_tests {
    use super::patch_change;

    #[test]
    fn a_patch_inside_a_script_gives_its_first_file() {
        // exec carries the patch in a script, where its line breaks are the two characters `\n`.
        let script = r#"const patch = "*** Begin Patch\n*** Delete File: E:/z-ui/xin-chao.txt\n*** End Patch"; run(patch)"#;
        let (name, change) = patch_change(script).expect("a delete is found");
        assert_eq!((name, change.path.as_str()), ("Delete", "E:/z-ui/xin-chao.txt"));
    }

    #[test]
    fn an_added_file_counts_its_lines() {
        let patch = "*** Begin Patch\n*** Add File: a.txt\n+one\n+two\n*** End Patch";
        let (name, change) = patch_change(patch).expect("an add is found");
        assert_eq!((name, change.added, change.removed), ("Write", 2, 0));
    }
}
