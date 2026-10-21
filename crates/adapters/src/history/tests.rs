use std::{fs, path::PathBuf};

use serde_json::{json, Value};

use super::*;

/// A throwaway `~/.claude` / `~/.codex` layout. Fixtures are synthetic, never real chats.
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ag-history-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
    fn roots(&self) -> Roots {
        Roots::from_home(&self.0)
    }
    fn write(&self, relative: &str, lines: &[Value]) -> PathBuf {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let body: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
        fs::write(&path, body.join("\n")).unwrap();
        path
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn user(uuid: &str, parent: Option<&str>, content: Value, ts: &str) -> Value {
    json!({"type":"user","uuid":uuid,"parentUuid":parent,"timestamp":ts,"cwd":"C:\\work\\demo","isSidechain":false,
           "message":{"role":"user","content":content}})
}

fn assistant(uuid: &str, parent: &str, content: Value) -> Value {
    json!({"type":"assistant","uuid":uuid,"parentUuid":parent,"timestamp":"2026-01-01T00:00:05Z","isSidechain":false,
           "message":{"role":"assistant","content":content}})
}

fn responses(messages: &[HistoryMessage]) -> Vec<String> {
    messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            HistoryBlock::Response { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn claude_transcript_merges_assistant_lines_and_attaches_tool_results() {
    let home = Home::new("claude-transcript");
    let path = home.write(
        ".claude/projects/demo/11111111.jsonl",
        &[
            user("a", None, json!("hello <system-reminder>hidden</system-reminder>"), "2026-01-01T00:00:00Z"),
            assistant(
                "b",
                "a",
                json!([
                    {"type":"thinking","thinking":"plan"},
                    {"type":"text","text":"hi"},
                    {"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}
                ]),
            ),
            user(
                "c",
                Some("b"),
                json!([{"type":"tool_result","tool_use_id":"t1","content":"file1","is_error":false}]),
                "2026-01-01T00:00:06Z",
            ),
            assistant("d", "c", json!([{"type":"text","text":"done"}])),
            json!({"type":"assistant","uuid":"s","parentUuid":"d","isSidechain":true,
                   "message":{"content":[{"type":"text","text":"subagent"}]}}),
            json!({"type":"ai-title","aiTitle":"Demo chat","sessionId":"11111111"}),
        ],
    );

    let messages = load(&home.roots(), Source::Claude, &path).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, "user");
    assert_eq!(
        messages[0].blocks,
        vec![HistoryBlock::Response {
            text: "hello".into()
        }]
    );
    assert_eq!(
        messages[1].blocks,
        vec![
            HistoryBlock::Thinking {
                text: "plan".into()
            },
            HistoryBlock::Response { text: "hi".into() },
            HistoryBlock::Command {
                command: "Bash  ls".into(),
                output: "file1".into(),
                status: "ok".into()
            },
            HistoryBlock::Response {
                text: "done".into()
            },
        ]
    );
}

#[test]
fn claude_keeps_older_messages_when_the_parent_link_is_gone() {
    let home = Home::new("claude-compacted");
    let path = home.write(
        ".claude/projects/demo/44444444.jsonl",
        &[
            user("a", None, json!("old question"), "2026-01-01T00:00:00Z"),
            assistant("b", "a", json!([{"type":"text","text":"old answer"}])),
            // After compaction the next message points at a uuid that is no longer in the file.
            user(
                "c",
                Some("gone"),
                json!("new question"),
                "2026-01-01T01:00:00Z",
            ),
            assistant("d", "c", json!([{"type":"text","text":"new answer"}])),
        ],
    );
    let messages = load(&home.roots(), Source::Claude, &path).unwrap();
    assert_eq!(
        responses(&messages),
        vec!["old question", "old answer", "new question", "new answer"]
    );
}

#[test]
fn terminal_colour_codes_are_removed_from_tool_output() {
    let home = Home::new("claude-ansi");
    let path = home.write(
        ".claude/projects/demo/55555555.jsonl",
        &[
            user("a", None, json!("run"), "2026-01-01T00:00:00Z"),
            assistant(
                "b",
                "a",
                json!([{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"lint"}}]),
            ),
            user(
                "c",
                Some("b"),
                json!([{"type":"tool_result","tool_use_id":"t1","content":"[32mOK[0m done"}]),
                "t",
            ),
        ],
    );
    let messages = load(&home.roots(), Source::Claude, &path).unwrap();
    assert_eq!(
        messages[1].blocks,
        vec![HistoryBlock::Command {
            command: "Bash  lint".into(),
            output: "OK done".into(),
            status: "ok".into()
        }]
    );
}

#[test]
fn claude_failed_tool_result_is_marked() {
    let home = Home::new("claude-fail");
    let path = home.write(
        ".claude/projects/demo/22222222.jsonl",
        &[
            user("a", None, json!("go"), "2026-01-01T00:00:00Z"),
            assistant("b", "a", json!([{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"x.rs"}}])),
            user(
                "c",
                Some("b"),
                json!([{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"nope"}],"is_error":true}]),
                "t",
            ),
        ],
    );
    let messages = load(&home.roots(), Source::Claude, &path).unwrap();
    assert_eq!(
        messages[1].blocks,
        vec![HistoryBlock::Command {
            command: "Read  x.rs".into(),
            output: "nope".into(),
            status: "fail".into()
        }]
    );
}

#[test]
fn claude_rewind_follows_the_newest_branch() {
    let home = Home::new("claude-rewind");
    let path = home.write(
        ".claude/projects/demo/33333333.jsonl",
        &[
            user("a", None, json!("first"), "2026-01-01T00:00:00Z"),
            assistant("b", "a", json!([{"type":"text","text":"old answer"}])),
            // The user rewound and asked again from "a": a second child of the same parent.
            user("e", Some("a"), json!("second"), "2026-01-01T00:01:00Z"),
            assistant("f", "e", json!([{"type":"text","text":"new answer"}])),
        ],
    );
    let messages = load(&home.roots(), Source::Claude, &path).unwrap();
    assert_eq!(responses(&messages), vec!["first", "second", "new answer"]);
}

#[test]
fn claude_index_reads_title_cwd_and_links_forks() {
    let home = Home::new("claude-fork");
    let shared = [
        user("a", None, json!("start"), "2026-01-01T00:00:00Z"),
        assistant("b", "a", json!([{"type":"text","text":"ok"}])),
    ];
    let mut original = shared.to_vec();
    original.push(json!({"type":"custom-title","customTitle":"Original","sessionId":"x"}));
    home.write(".claude/projects/demo/aaaaaaaa.jsonl", &original);

    let fork = vec![
        user("a", None, json!("start"), "2026-01-02T00:00:00Z"),
        assistant("b", "a", json!([{"type":"text","text":"ok"}])),
        user("z", Some("b"), json!("diverge"), "2026-01-02T00:01:00Z"),
        json!({"type":"ai-title","aiTitle":"Forked","sessionId":"y"}),
    ];
    home.write(".claude/projects/demo/bbbbbbbb.jsonl", &fork);
    // Subagent files are not sessions.
    home.write(".claude/projects/demo/agent-123.jsonl", &shared);

    let sessions = list_all(&home.roots());
    assert_eq!(sessions.len(), 2);
    let root = sessions.iter().find(|s| s.title == "Original").unwrap();
    let child = sessions.iter().find(|s| s.title == "Forked").unwrap();
    assert_eq!(root.cwd, "C:\\work\\demo");
    assert_eq!(root.parent_id, None);
    assert_eq!(child.parent_id.as_deref(), Some("claude:aaaaaaaa"));
    assert_eq!(child.fork_seq, Some(2));
}

#[test]
fn codex_transcript_skips_injected_context_and_reads_tools() {
    let home = Home::new("codex");
    let msg = |role: &str, kind: &str, text: &str| {
        json!({"timestamp":"2026-01-01T00:00:01Z","type":"response_item",
               "payload":{"type":"message","role":role,"content":[{"type":kind,"text":text}]}})
    };
    let path = home.write(
        ".codex/sessions/2026/01/01/rollout-2026-01-01T00-00-00-abc123.jsonl",
        &[
            json!({"timestamp":"2026-01-01T00:00:00Z","type":"session_meta",
                   "payload":{"id":"abc123","cwd":"C:\\work\\demo","timestamp":"2026-01-01T00:00:00Z"}}),
            msg("user", "input_text", "<environment_context>cwd</environment_context>"),
            msg("user", "input_text", "list files"),
            json!({"timestamp":"t","type":"response_item",
                   "payload":{"type":"reasoning","summary":[{"type":"summary_text","text":"thinking about ls"}]}}),
            json!({"timestamp":"t","type":"response_item",
                   "payload":{"type":"function_call","name":"shell","arguments":"{\"command\":[\"bash\",\"-lc\",\"ls\"]}","call_id":"c1"}}),
            json!({"timestamp":"t","type":"response_item",
                   "payload":{"type":"function_call_output","call_id":"c1","output":"{\"output\":\"boom\",\"metadata\":{\"exit_code\":1}}"}}),
            msg("assistant", "output_text", "it failed"),
            json!({"timestamp":"t","type":"event_msg","payload":{"type":"token_count"}}),
        ],
    );

    let sessions = list_all(&home.roots());
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "codex:abc123");
    assert_eq!(sessions[0].title, "list files");
    assert_eq!(sessions[0].cwd, "C:\\work\\demo");
    assert_eq!(sessions[0].message_count, 2);

    let messages = load(&home.roots(), Source::Codex, &path).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].blocks,
        vec![HistoryBlock::Response {
            text: "list files".into()
        }]
    );
    assert_eq!(
        messages[1].blocks,
        vec![
            HistoryBlock::Thinking {
                text: "thinking about ls".into()
            },
            HistoryBlock::Command {
                command: "shell  bash -lc ls".into(),
                output: "boom".into(),
                status: "fail".into()
            },
            HistoryBlock::Response {
                text: "it failed".into()
            },
        ]
    );
}

#[test]
fn load_refuses_paths_outside_the_history_folders() {
    let home = Home::new("outside");
    fs::create_dir_all(home.roots().claude).unwrap();
    let outside = home.write("notes/secret.jsonl", &[json!({"type":"user"})]);
    assert!(matches!(
        load(&home.roots(), Source::Claude, &outside),
        Err(HistoryError::OutsideRoots)
    ));
}

#[test]
fn missing_folders_yield_no_sessions() {
    let home = Home::new("empty");
    assert!(list_all(&home.roots()).is_empty());
}
