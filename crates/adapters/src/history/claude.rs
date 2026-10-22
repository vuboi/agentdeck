//! Claude Code sessions: `~/.claude/projects/<cwd with separators as "-">/<session-uuid>.jsonl`.
//!
//! One file per session. Every message line has `uuid` and `parentUuid`, so a conversation is a
//! tree; the transcript is the chain from the newest message back to the root. Subagent runs live
//! in `agent-*.jsonl` files and are ignored.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use serde_json::Value;

use super::util::{
    describe_tool, jsonl_files, read_lines, result_text, strip_ansi, strip_reminders, truncate,
};
use super::{HistoryBlock, HistoryMessage, HistorySession, HistoryUsage, Source};

/// Message uuids kept per session to recognise forks and resumes.
const SHARED_PREFIX: usize = 12;
const OUTPUT_LIMIT: usize = 6_000;

/// Lines that are never messages and can be huge; not worth parsing.
fn noisy(line: &str) -> bool {
    [
        "\"type\":\"api-request",
        "\"type\":\"attachment\"",
        "\"type\":\"file-history",
        "\"type\":\"queue-operation\"",
    ]
    .iter()
    .any(|marker| line.contains(marker))
}

struct Indexed {
    session: HistorySession,
    first_uuids: Vec<String>,
}

pub(super) fn list(root: &Path) -> Vec<HistorySession> {
    let mut indexed: Vec<Indexed> = jsonl_files(root)
        .into_iter()
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("agent-"))
        })
        .filter_map(|path| index_file(&path))
        .collect();
    link_forks(&mut indexed);
    indexed.into_iter().map(|item| item.session).collect()
}

fn index_file(path: &Path) -> Option<Indexed> {
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let lines = read_lines(path, |line| !noisy(line)).ok()?;

    let (mut custom, mut ai, mut first_user) = (None, None, None);
    let (mut cwd, mut started, mut updated) = (String::new(), None, None);
    let (mut count, mut first_uuids) = (0usize, Vec::new());

    for line in &lines {
        match line.get("type").and_then(Value::as_str) {
            Some("custom-title") => {
                custom = line
                    .get("customTitle")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            }
            Some("ai-title") => {
                ai = line
                    .get("aiTitle")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            }
            Some(kind @ ("user" | "assistant")) if !is_true(line, "isSidechain") => {
                if is_true(line, "isMeta") {
                    continue;
                }
                count += 1;
                if cwd.is_empty() {
                    cwd = str_of(line, "cwd").to_owned();
                }
                let stamp = str_of(line, "timestamp");
                if !stamp.is_empty() {
                    started.get_or_insert_with(|| stamp.to_owned());
                    updated = Some(stamp.to_owned());
                }
                if first_uuids.len() < SHARED_PREFIX {
                    first_uuids.push(str_of(line, "uuid").to_owned());
                }
                if kind == "user" && first_user.is_none() {
                    first_user = user_text(line)
                        .filter(|text| !text.starts_with('<'))
                        .map(|text| truncate(&text, 80));
                }
            }
            _ => {}
        }
    }
    if count == 0 {
        return None;
    }

    let title = custom
        .or(ai)
        .or(first_user)
        .unwrap_or_else(|| "Untitled session".to_owned());
    Some(Indexed {
        session: HistorySession {
            id: format!("claude:{stem}"),
            source: Source::Claude,
            title,
            model: None,
            cwd,
            started_at: started,
            updated_at: updated,
            message_count: count,
            path: path.to_string_lossy().into_owned(),
            parent_id: None,
            fork_seq: None,
        },
        first_uuids,
    })
}

/// A session that starts with the same message uuids as an earlier one was forked or resumed from it.
/// The parent is the closest earlier session sharing the first message.
fn link_forks(indexed: &mut [Indexed]) {
    let mut by_first: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, item) in indexed.iter().enumerate() {
        if let Some(first) = item.first_uuids.first().filter(|uuid| !uuid.is_empty()) {
            by_first.entry(first.clone()).or_default().push(index);
        }
    }
    for members in by_first.values().filter(|members| members.len() > 1) {
        let mut ordered = members.clone();
        ordered.sort_by(|a, b| {
            indexed[*a]
                .session
                .started_at
                .cmp(&indexed[*b].session.started_at)
        });
        for pair in ordered.windows(2) {
            let (parent, child) = (pair[0], pair[1]);
            let shared = indexed[parent]
                .first_uuids
                .iter()
                .zip(&indexed[child].first_uuids)
                .take_while(|(a, b)| a == b)
                .count();
            let parent_id = indexed[parent].session.id.clone();
            let session = &mut indexed[child].session;
            session.parent_id = Some(parent_id);
            session.fork_seq = Some(shared);
        }
    }
}

struct Entry {
    uuid: String,
    parent: String,
    kind: String,
    value: Value,
}

pub(super) fn load(path: &Path) -> std::io::Result<Vec<HistoryMessage>> {
    let lines = read_lines(path, |line| !noisy(line))?;
    let entries: Vec<Entry> = lines
        .into_iter()
        .filter(|line| !str_of(line, "uuid").is_empty())
        .map(|line| Entry {
            uuid: str_of(&line, "uuid").to_owned(),
            parent: str_of(&line, "parentUuid").to_owned(),
            kind: str_of(&line, "type").to_owned(),
            value: line,
        })
        .collect();
    let position: HashMap<&str, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (e.uuid.as_str(), i))
        .collect();

    // The transcript is the chain from the newest real message back to the root.
    let is_message = |entry: &Entry| {
        matches!(entry.kind.as_str(), "user" | "assistant") && !is_true(&entry.value, "isSidechain")
    };
    let Some(leaf) = entries.iter().rposition(is_message) else {
        return Ok(Vec::new());
    };
    let mut chain = Vec::new();
    let mut cursor = Some(leaf);
    while let Some(index) = cursor {
        if chain.contains(&index) {
            break; // damaged file: a cycle
        }
        chain.push(index);
        cursor = position.get(entries[index].parent.as_str()).copied();
    }
    chain.reverse();

    // Context compaction can leave the first kept message pointing at a parent that is no longer in
    // the file. Then the chain is only the tail of the chat, so keep the older messages before it.
    if let Some(&head) = chain.first() {
        let parent = entries[head].parent.as_str();
        if !parent.is_empty() && !position.contains_key(parent) {
            let mut older: Vec<usize> = (0..head).filter(|i| is_message(&entries[*i])).collect();
            older.extend(chain);
            chain = older;
        }
    }

    let mut messages: Vec<HistoryMessage> = Vec::new();
    // tool_use id -> (message index, block index), to attach the result later.
    let mut pending: HashMap<String, (usize, usize)> = HashMap::new();
    // One model call is written as several lines (text, thinking, each tool call), all with the call's usage:
    // count each call once, by its message id. `carry` holds what is not yet on an answer.
    let mut counted: HashSet<String> = HashSet::new();
    let mut carry: Option<HistoryUsage> = None;
    let mut model: Option<String> = None;

    for index in chain {
        let entry = &entries[index];
        if !is_message(entry) || is_true(&entry.value, "isMeta") {
            continue;
        }
        let role = entry.kind.as_str();
        let time = str_of(&entry.value, "timestamp").to_owned();
        if role == "assistant" {
            let message = entry.value.get("message");
            let call = message.and_then(|m| m.get("id")).and_then(Value::as_str).unwrap_or_default();
            if let Some(usage) = message.and_then(|m| m.get("usage")) {
                if call.is_empty() || counted.insert(call.to_owned()) {
                    let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
                    HistoryUsage::merge(
                        &mut carry,
                        Some(HistoryUsage {
                            input: count("input_tokens"),
                            output: count("output_tokens"),
                            cache_read: count("cache_read_input_tokens"),
                            cache_write: count("cache_creation_input_tokens"),
                        }),
                    );
                }
            }
            if let Some(name) = message.and_then(|m| m.get("model")).and_then(Value::as_str) {
                if !name.starts_with('<') {
                    model = Some(name.to_owned());
                }
            }
        }
        let content = entry.value.get("message").and_then(|m| m.get("content"));
        let mut blocks = Vec::new();

        match content {
            Some(Value::String(text)) => push_text(&mut blocks, text, role == "user"),
            Some(Value::Array(items)) => {
                for item in items {
                    match item.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            push_text(&mut blocks, str_of(item, "text"), role == "user")
                        }
                        Some("thinking") => {
                            let text = str_of(item, "thinking");
                            if !text.is_empty() {
                                blocks.push(HistoryBlock::Thinking {
                                    text: text.to_owned(),
                                });
                            }
                        }
                        Some("tool_use") => {
                            let id = str_of(item, "id").to_owned();
                            let command = describe_tool(
                                str_of(item, "name"),
                                item.get("input").unwrap_or(&Value::Null),
                            );
                            blocks.push(HistoryBlock::Command {
                                command,
                                output: String::new(),
                                status: "ok".into(),
                            });
                            // Remember where it will land once the block list is attached to a message.
                            pending.insert(id, (usize::MAX, blocks.len() - 1));
                        }
                        Some("tool_result") => {
                            attach_result(&mut messages, &mut pending, item);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if blocks.is_empty() {
            continue;
        }

        // Consecutive assistant lines are one answer (text, thinking and tool calls arrive as separate lines).
        let merge =
            role == "assistant" && messages.last().is_some_and(|last| last.role == "assistant");
        let target = if merge {
            messages.len() - 1
        } else {
            messages.len()
        };
        let offset = if merge {
            messages[target].blocks.len()
        } else {
            0
        };
        for slot in pending.values_mut().filter(|slot| slot.0 == usize::MAX) {
            *slot = (target, slot.1 + offset);
        }
        if merge {
            messages[target].blocks.extend(blocks);
        } else {
            messages.push(HistoryMessage {
                id: entry.uuid.clone(),
                role: role.to_owned(),
                time,
                blocks,
                usage: None,
                model: None,
            });
        }
        if role == "assistant" {
            HistoryUsage::merge(&mut messages[target].usage, carry.take());
            if model.is_some() {
                messages[target].model = model.take();
            }
        }
    }
    messages.retain(|message| !message.blocks.is_empty());
    Ok(messages)
}

fn attach_result(
    messages: &mut [HistoryMessage],
    pending: &mut HashMap<String, (usize, usize)>,
    item: &Value,
) {
    let Some(&(message, block)) = pending.get(str_of(item, "tool_use_id")) else {
        return;
    };
    let Some(HistoryBlock::Command { output, status, .. }) = messages
        .get_mut(message)
        .and_then(|m| m.blocks.get_mut(block))
    else {
        return;
    };
    *output = truncate(
        &strip_ansi(&result_text(item.get("content").unwrap_or(&Value::Null))),
        OUTPUT_LIMIT,
    );
    if is_true(item, "is_error") {
        *status = "fail".into();
    }
}

fn push_text(blocks: &mut Vec<HistoryBlock>, text: &str, strip: bool) {
    let text = if strip {
        strip_reminders(text)
    } else {
        text.trim().to_owned()
    };
    if !text.is_empty() {
        blocks.push(HistoryBlock::Response { text });
    }
}

/// First plain text of a user line, without injected reminders.
fn user_text(line: &Value) -> Option<String> {
    let content = line.get("message")?.get("content")?;
    let raw = match content {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .find(|item| item.get("type").and_then(Value::as_str) == Some("text"))
            .map(|item| str_of(item, "text").to_owned())?,
        _ => return None,
    };
    let text = strip_reminders(&raw);
    (!text.is_empty()).then_some(text)
}

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn is_true(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}
