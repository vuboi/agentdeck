//! Drives the REAL Codex CLI (`codex app-server`) through AgentDeck's adapter. It calls the network and spends a little
//! of the signed-in account's quota, so every test is ignored. Run with:
//!
//! `cargo test -p ag-adapters --test real_codex -- --ignored --nocapture --test-threads=1`
//!
//! Set `AGENTDECK_CODEX_MODEL` to use another model (default `gpt-5.6-luna`, low effort).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use ag_adapters::agent::codex::Codex;
use ag_adapters::agent::interrupt::StopHandle;
use ag_adapters::agent::runner::resolve_binary;
use ag_adapters::agent::warm::{drive_warm_turn_with, inject, reset};
use ag_adapters::agent::{AgentEvent, Provider, TaskStatus, TurnRequest};

fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("agentdeck-real-codex").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

fn request(chat: &str, prompt: &str, cwd: &std::path::Path) -> TurnRequest {
    TurnRequest {
        prompt: prompt.into(),
        session_id: None,
        resume: false,
        model: Some(std::env::var("AGENTDECK_CODEX_MODEL").unwrap_or_else(|_| "gpt-5.6-luna".into())),
        permission_mode: Some("yolo".into()),
        effort: Some("low".into()),
        allowed_tools: Vec::new(),
        cwd: Some(cwd.to_path_buf()),
        owner: Some(chat.into()),
        api_key: None,
    }
}

fn show(event: &AgentEvent) -> String {
    match event {
        AgentEvent::Delta { text } => format!("Delta {text:?}"),
        AgentEvent::Thinking { text } => format!("Thinking {}", text.chars().take(40).collect::<String>()),
        AgentEvent::Scoped { parent, event } => format!("  Scoped<{parent}> {}", show(event)),
        other => format!("{other:?}").chars().take(220).collect(),
    }
}

/// What a scenario does while its turn runs.
#[derive(Default)]
struct Plan {
    /// After this long (once the first event came) send this message with `warm::inject`.
    inject: Option<(u64, &'static str)>,
    /// After this long (once the first event came) stop the turn.
    stop: Option<u64>,
}

async fn run(chat: &str, prompt: &str, cwd: &std::path::Path, plan: Plan) -> (Vec<AgentEvent>, Option<bool>) {
    let provider: &'static Codex = Box::leak(Box::new(Codex::default()));
    let binary = resolve_binary(provider);
    let req = request(chat, prompt, cwd);
    let (handle, token) = StopHandle::new();
    let events: Arc<Mutex<Vec<AgentEvent>>> = Arc::default();
    let seen = Arc::clone(&events);
    let turn = tokio::spawn(async move {
        drive_warm_turn_with(binary, provider as &dyn Provider, &req, token, |event| {
            println!("  {}", show(&event));
            seen.lock().expect("events").push(event);
        })
        .await
    });
    // The first sign of the model working (not the thread id line) starts the clocks.
    for _ in 0..600 {
        if events.lock().expect("events").iter().any(|event| !matches!(event, AgentEvent::Done { .. })) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let mut taken = None;
    if let Some((after, text)) = plan.inject {
        tokio::time::sleep(Duration::from_millis(after)).await;
        taken = Some(inject(chat, text));
        println!("  >> inject {text:?} taken={taken:?}");
    }
    if let Some(after) = plan.stop {
        tokio::time::sleep(Duration::from_millis(after)).await;
        println!("  >> stop");
        handle.stop();
    }
    let ended = tokio::time::timeout(Duration::from_secs(300), turn).await;
    assert!(matches!(ended, Ok(Ok(true))), "the turn did not end: {ended:?}");
    reset(Some("codex"));
    let list = events.lock().expect("events").clone();
    (list, taken)
}

fn done_count(events: &[AgentEvent]) -> usize {
    events.iter().filter(|event| matches!(event, AgentEvent::Done { .. })).count()
}

#[tokio::test]
#[ignore = "calls the network with the signed-in Codex account"]
async fn plain_answer_ends_with_one_done() {
    let dir = folder("plain");
    let (events, _) = run("real-plain", "Reply with exactly the word PONG and nothing else.", &dir, Plan::default()).await;
    let text: String = events.iter().filter_map(|event| if let AgentEvent::Delta { text } = event { Some(text.as_str()) } else { None }).collect();
    assert!(text.contains("PONG"), "answer: {text:?}");
    assert_eq!(done_count(&events), 1);
    assert!(matches!(events.last(), Some(AgentEvent::Done { session_id: Some(_), .. })), "the turn names its thread");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::Usage { .. })), "usage is reported");
}

#[tokio::test]
#[ignore = "calls the network with the signed-in Codex account"]
async fn a_tool_turn_shows_calls_with_results_and_a_file_diff() {
    let dir = folder("tools");
    let (events, _) = run("real-tools", "Create a file hello.txt containing the single line hi, then run the command `echo ok`, then answer DONE.", &dir, Plan::default()).await;
    let calls: Vec<&AgentEvent> = events.iter().filter(|event| matches!(event, AgentEvent::ToolCall { .. })).collect();
    assert!(!calls.is_empty(), "no tool call");
    for call in &calls {
        let AgentEvent::ToolCall { id, .. } = call else { continue };
        let results = events.iter().filter(|event| matches!(event, AgentEvent::ToolResult { id: other, .. } if other == id)).count();
        assert!(results >= 1, "call {id} has no result");
    }
    let mut ids: Vec<&String> = calls.iter().filter_map(|call| if let AgentEvent::ToolCall { id, .. } = call { Some(id) } else { None }).collect();
    let before = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), before, "tool call ids are unique");
    assert!(dir.join("hello.txt").exists());
    assert_eq!(done_count(&events), 1);
}

#[tokio::test]
#[ignore = "calls the network with the signed-in Codex account"]
async fn parallel_sub_agents_stay_under_their_spawn_calls_and_never_end_the_turn() {
    let dir = folder("subagents");
    let prompt = "Spawn exactly two sub-agents in parallel with the spawn_agent tool. Agent one must reply only with the word ALPHA. Agent two must reply only with the word BRAVO. Then wait for both and answer with: GOT ALPHA BRAVO.";
    let (events, _) = run("real-subagents", prompt, &dir, Plan::default()).await;
    assert_eq!(done_count(&events), 1, "a child's turn/completed must not end the parent turn");
    assert!(matches!(events.last(), Some(AgentEvent::Done { .. })), "Done is the last event");
    let spawns: Vec<String> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::ToolCall { id, name, .. } if name == "spawnAgent" => Some(id.clone()),
            _ => None,
        })
        .collect();
    assert!(!spawns.is_empty(), "the model did not spawn a sub-agent (check the log above)");
    for event in &events {
        if let AgentEvent::Scoped { parent, .. } = event {
            assert!(spawns.contains(parent), "scoped under an unknown call {parent}");
        }
    }
    let started = events.iter().filter(|event| matches!(event, AgentEvent::Task { status: TaskStatus::Started, .. })).count();
    let ended = events.iter().filter(|event| matches!(event, AgentEvent::Task { status: TaskStatus::Done | TaskStatus::Failed, .. })).count();
    assert!(started >= 1 && ended >= 1, "task events: started {started}, ended {ended}");
    let top: String = events.iter().filter_map(|event| if let AgentEvent::Delta { text } = event { Some(text.as_str()) } else { None }).collect();
    println!("  main answer: {top:?}");
}

#[tokio::test]
#[ignore = "calls the network with the signed-in Codex account"]
async fn a_message_sent_mid_turn_reaches_the_running_turn() {
    let dir = folder("steer");
    let prompt = "Run the command `sleep 12` (wait for it), then answer with the single word FIRST.";
    let plan = Plan { inject: Some((1500, "Also, end your answer with the word SECOND.")), stop: None };
    let (events, taken) = run("real-steer", prompt, &dir, plan).await;
    assert_eq!(taken, Some(true), "Codex takes a message while its turn runs (turn/steer)");
    let text: String = events.iter().filter_map(|event| if let AgentEvent::Delta { text } = event { Some(text.as_str()) } else { None }).collect();
    println!("  answer: {text:?}");
    assert!(text.to_uppercase().contains("SECOND"), "the answer covers the message sent mid-turn: {text:?}");
    assert_eq!(done_count(&events), 1, "merged into the same turn");
}

#[tokio::test]
#[ignore = "calls the network with the signed-in Codex account"]
async fn a_stop_ends_the_turn_as_stopped_and_the_server_stays_usable() {
    let dir = folder("stop");
    let plan = Plan { inject: None, stop: Some(2500) };
    let (events, _) = run("real-stop", "Run the command `sleep 60` and wait for it.", &dir, plan).await;
    let last = events.last().expect("events");
    assert!(
        matches!(last, AgentEvent::Done { .. }),
        "an interrupt closes the turn as a stopped Done, not an error: {last:?}"
    );
    assert!(!events.iter().any(|event| matches!(event, AgentEvent::Error { message } if message != "stopped")), "no failure shown");
}
