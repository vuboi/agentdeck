use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use serde_json::Value;

/// Lines longer than this are request blobs, never messages; skip them without parsing.
const MAX_LINE: usize = 2_000_000;

/// All `*.jsonl` files below `root`, any depth. A missing folder yields nothing.
pub fn jsonl_files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                found.push(path);
            }
        }
    }
    found
}

/// Parsed JSON lines of a file. Bad lines, oversize lines and lines for which `wanted` is false
/// are skipped, so a damaged or huge file still loads.
pub fn read_lines(path: &Path, wanted: impl Fn(&str) -> bool) -> std::io::Result<Vec<Value>> {
    let reader = BufReader::new(File::open(path)?);
    let mut out = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.is_empty() || line.len() > MAX_LINE || !wanted(&line) {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            out.push(value);
        }
    }
    Ok(out)
}

/// At most `max` characters, with an ellipsis when cut.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

/// Removes `<system-reminder>…</system-reminder>` blocks the CLIs inject into user turns.
pub fn strip_reminders(text: &str) -> String {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        match rest[start..].find(CLOSE) {
            Some(end) => rest = &rest[start + end + CLOSE.len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_owned()
}

/// Text of a tool result: a plain string, or an array of `{ "type": "text", "text": … }`.
pub fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// One-line summary of a tool call: `Name  argument`.
pub fn describe_tool(name: &str, input: &Value) -> String {
    if matches!(name, "TodoWrite" | "update_plan") {
        if let Some(plan) = crate::agent::plan_json(input) {
            return format!("{name}  {plan}");
        }
    }
    // A field is a string or an array of strings (Codex shell calls: ["bash", "-lc", "ls"]).
    let pick = |keys: &[&str]| {
        keys.iter().find_map(|key| match input.get(*key)? {
            Value::String(text) => Some(text.clone()),
            Value::Array(parts) => Some(
                parts
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            _ => None,
        })
    };
    let arg = match name {
        "Bash" | "shell" | "exec_command" => pick(&["command", "cmd"]),
        "Read" | "Edit" | "Write" | "NotebookEdit" => pick(&["file_path", "path"]),
        "Grep" | "Glob" => pick(&["pattern"]),
        "Agent" | "Task" => pick(&["description", "prompt"]),
        _ => None,
    }
    .or_else(|| {
        let compact = input.to_string();
        (compact != "null" && compact != "{}").then_some(compact)
    })
    .unwrap_or_default();
    truncate(format!("{name}  {arg}").trim(), 240)
}

/// Removes terminal colour codes (`ESC [ … m` and friends) that tool output carries.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '' {
            out.push(ch);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            // A CSI sequence ends at the first byte in 0x40..=0x7E.
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        }
    }
    out
}
