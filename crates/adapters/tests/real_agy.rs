//! Drives the REAL Antigravity CLI (`agy`) through the adapter, the way the app does (`drive_warm_turn_with`), and prints
//! what the app would receive. Needs `agy` installed and signed in; spends a few cheap model calls and takes minutes
//! (one scenario waits 60 s to see that a kept process survives idling). Never runs by default:
//!
//! ```text
//! cargo test -p ag-adapters --test real_agy -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ag_adapters::agent::antigravity::Antigravity;
use ag_adapters::agent::interrupt::{StopHandle, StopToken};
use ag_adapters::agent::resolve::resolve_binary;
use ag_adapters::agent::warm::{drive_warm_turn_with, reset};
use ag_adapters::agent::{AgentEvent, Provider, TaskStatus, TurnRequest};

const MODEL: &str = "gemini-3.8-flash-low";
const TURN_LIMIT: Duration = Duration::from_secs(240);

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("agentdeck-real-agy-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

fn request(chat: &str, prompt: &str, conversation: Option<&str>, mode: Option<&str>, cwd: &PathBuf) -> TurnRequest {
    TurnRequest {
        prompt: prompt.into(),
        session_id: conversation.map(str::to_owned),
        resume: conversation.is_some(),
        model: Some(MODEL.into()),
        permission_mode: mode.map(str::to_owned),
        effort: None,
        allowed_tools: Vec::new(),
        cwd: Some(cwd.clone()),
        owner: Some(chat.into()),
        api_key: None,
    }
}

/// One turn as the app runs it: a provider of its own, the events collected, optionally stopped after `stop_after`.
async fn turn(req: TurnRequest, stop_after: Option<Duration>) -> Vec<AgentEvent> {
    let provider = Antigravity::default();
    let binary = resolve_binary(&provider);
    let (handle, token) = StopHandle::new();
    let token: StopToken = if stop_after.is_some() { token } else { StopToken::never() };
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&events);
    let run = async {
        drive_warm_turn_with(binary, &provider as &dyn Provider, &req, token, |event| seen.lock().unwrap().push(event)).await
    };
    let stopper = async {
        match stop_after {
            Some(after) => {
                tokio::time::sleep(after).await;
                handle.stop();
                std::future::pending::<()>().await
            }
            None => std::future::pending::<()>().await,
        }
    };
    let ran = tokio::select! {
        ran = tokio::time::timeout(TURN_LIMIT, run) => ran,
        () = stopper => unreachable!(),
    };
    assert!(matches!(ran, Ok(true)), "the turn did not run to its end: {ran:?}");
    let list = events.lock().unwrap().clone();
    list
}

fn show(title: &str, events: &[AgentEvent]) {
    println!("\n=== {title}");
    for event in events {
        let line = match event {
            AgentEvent::Delta { text } => format!("delta {text:?}"),
            AgentEvent::Thinking { text } => format!("thinking {} chars", text.len()),
            AgentEvent::ToolCall { id, name, input, background, .. } => format!("tool_call #{id} {name} {input:.80?} background={background}"),
            AgentEvent::ToolResult { id, output, is_error, .. } => format!("tool_result #{id} error={is_error} {output:.80?}"),
            AgentEvent::PermissionDenied { requests } => format!("permission_denied {:?}", requests.iter().map(|r| (&r.tool, &r.summary)).collect::<Vec<_>>()),
            AgentEvent::Done { session_id, text, .. } => format!("DONE session={session_id:?} text={text:?}"),
            AgentEvent::Error { message } => format!("ERROR {message:?}"),
            AgentEvent::Task { id, tool_use_id, status, label, summary, tools } => format!("task {id} call={tool_use_id:?} {status:?} label={label:?} summary={summary:?} tools={tools:?}"),
            AgentEvent::Usage { .. } => continue,
            other => format!("{other:?}"),
        };
        println!("  {line}");
    }
}

fn answer(events: &[AgentEvent]) -> String {
    events.iter().filter_map(|event| if let AgentEvent::Delta { text } = event { Some(text.as_str()) } else { None }).collect()
}

fn conversation(events: &[AgentEvent]) -> Option<String> {
    events.iter().find_map(|event| if let AgentEvent::Done { session_id: Some(id), .. } = event { Some(id.clone()) } else { None })
}

fn dones(events: &[AgentEvent]) -> usize {
    events.iter().filter(|event| matches!(event, AgentEvent::Done { .. })).count()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "drives the real agy CLI: needs it installed and signed in, spends model calls, takes minutes"]
async fn real_agy_through_the_adapter() {
    // (1) A plain answer: deltas, one Done with the conversation id.
    let dir = folder("plain");
    let first = turn(request("agy-plain", "Reply with only the word PONG.", None, None, &dir), None).await;
    show("1 plain", &first);
    assert!(answer(&first).contains("PONG"), "no answer");
    assert_eq!(dones(&first), 1);
    let id = conversation(&first).expect("the Done carries agy's conversation id");

    // (4) The next message of the chat reuses the kept process and the conversation: two clean turns, nothing merged.
    let second = turn(request("agy-plain", "What word did I ask you to reply with before? Reply with only that word.", Some(&id), None, &dir), None).await;
    show("4 second message on the same conversation", &second);
    assert_eq!(dones(&second), 1);
    assert!(answer(&second).contains("PONG"), "the second turn lost the conversation");
    assert!(!answer(&second).contains("PONGPONG"));
    reset(Some("antigravity"));

    // (2) A tool turn in the default mode (asks): the app should get a permission request, not a hang.
    let dir = folder("tools-ask");
    let asked = turn(request("agy-ask", "Create a file named note.txt containing hello, then run the command `echo hi` and tell me what it printed.", None, None, &dir), None).await;
    show("2a tools, default permission mode", &asked);
    println!("  note.txt written: {}", dir.join("note.txt").exists());
    reset(Some("antigravity"));

    // The same in yolo mode: calls and results pair up, the file exists.
    let dir = folder("tools-yolo");
    let done = turn(request("agy-yolo", "Create a file named note.txt containing hello, then run the command `echo hi` and tell me what it printed.", None, Some("bypassPermissions"), &dir), None).await;
    show("2b tools, yolo", &done);
    let calls = done.iter().filter(|event| matches!(event, AgentEvent::ToolCall { .. })).count();
    let results = done.iter().filter(|event| matches!(event, AgentEvent::ToolResult { .. })).count();
    assert!(calls > 0 && calls == results, "calls {calls} results {results}");
    assert!(dir.join("note.txt").exists(), "the file was not written");
    reset(Some("antigravity"));

    // (3) A sub-agent: a background call, a Task that names it, and none of its words in the answer.
    let dir = folder("subagent");
    let sub = turn(request("agy-sub", "Use a subagent (invoke_subagent) to compute 17*3. Relay its answer and reply with only the number.", None, Some("bypassPermissions"), &dir), None).await;
    show("3 sub-agent", &sub);
    assert!(sub.iter().any(|event| matches!(event, AgentEvent::ToolCall { background: true, .. })), "the sub-agent call is not marked as background work");
    assert!(sub.iter().any(|event| matches!(event, AgentEvent::Task { status: TaskStatus::Started, .. })));
    assert!(sub.iter().any(|event| matches!(event, AgentEvent::Task { status: TaskStatus::Done, .. })));
    assert!(answer(&sub).trim().ends_with("51"), "answer: {:?}", answer(&sub));
    assert!(answer(&sub).trim().len() < 12, "the sub-agent's text leaked into the answer: {:?}", answer(&sub));
    assert_eq!(dones(&sub), 1);
    reset(Some("antigravity"));

    // (5) A command left running in the background: the turn ends with its result and nothing follows. Is the job
    // still alive while the process is kept, and does it survive the process being dropped?
    let dir = folder("background");
    let marker = dir.join("bg-kept.txt");
    let prompt = format!(
        "Start this as a BACKGROUND command and do not wait for it: `Start-Sleep -Seconds 20; Set-Content -Path '{}' -Value finished`. Then reply with only the word STARTED.",
        marker.display()
    );
    let started = turn(request("agy-bg", &prompt, None, Some("bypassPermissions"), &dir), None).await;
    show("5a background command, process kept", &started);
    assert_eq!(dones(&started), 1);
    tokio::time::sleep(Duration::from_secs(30)).await;
    println!("  job finished while the process stayed warm: {}", marker.exists());
    reset(Some("antigravity"));
    let dropped_marker = dir.join("bg-dropped.txt");
    let prompt = format!(
        "Start this as a BACKGROUND command and do not wait for it: `Start-Sleep -Seconds 20; Set-Content -Path '{}' -Value finished`. Then reply with only the word STARTED.",
        dropped_marker.display()
    );
    let started = turn(request("agy-bg2", &prompt, None, Some("bypassPermissions"), &dir), None).await;
    assert_eq!(dones(&started), 1);
    reset(Some("antigravity"));
    tokio::time::sleep(Duration::from_secs(30)).await;
    println!("  job finished after the process was dropped right away: {}", dropped_marker.exists());

    // (6) A stop mid-turn is a kill (agy has no interrupt message). What does the app get, and can the next message
    // continue the conversation?
    let dir = folder("stop");
    let primed = turn(request("agy-stop", "Remember the secret word TULIP. Reply with only OK.", None, None, &dir), None).await;
    let id = conversation(&primed).expect("a conversation id after the first turn");
    let stopped = turn(request("agy-stop", "Run the command `Start-Sleep -Seconds 60` and then reply DONE.", Some(&id), Some("bypassPermissions"), &dir), Some(Duration::from_secs(8))).await;
    show("6 stopped mid-turn", &stopped);
    assert!(stopped.iter().any(|event| matches!(event, AgentEvent::Done { .. } | AgentEvent::Error { .. })), "a stop must end the turn with an event");
    let after = turn(request("agy-stop", "What was the secret word? Reply with only the word.", Some(&id), None, &dir), None).await;
    show("6 the message after the stop", &after);
    println!("  conversation survived the kill: {}", answer(&after).to_uppercase().contains("TULIP"));
    reset(Some("antigravity"));

    // (7) A kept process idles for 60 s and still takes the next turn.
    let dir = folder("idle");
    let one = turn(request("agy-idle", "Reply with only ONE.", None, None, &dir), None).await;
    let id = conversation(&one).expect("a conversation id");
    tokio::time::sleep(Duration::from_secs(60)).await;
    let two = turn(request("agy-idle", "Reply with only TWO.", Some(&id), None, &dir), None).await;
    show("7 after 60 s idle", &two);
    assert!(answer(&two).contains("TWO"));
    reset(Some("antigravity"));
}
