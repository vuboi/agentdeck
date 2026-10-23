//! Real end-to-end check of the Codex turn runner: a first turn, then a second one resuming its thread.
//! Uses the account's Codex quota. Run: `cargo run -p ag-adapters --example codex_turn`

use ag_adapters::agent::codex::Codex;
use ag_adapters::agent::runner::{drive_turn, resolve_binary};
use ag_adapters::agent::{AgentEvent, TurnRequest};

async fn turn(prompt: &str, thread: Option<String>, cwd: &std::path::Path) -> Vec<AgentEvent> {
    let codex = Codex::default();
    let request = TurnRequest {
        prompt: prompt.into(),
        resume: thread.is_some(),
        session_id: thread,
        model: Some("gpt-5.6-luna".into()),
        permission_mode: None,
        effort: Some("low".into()),
        allowed_tools: Vec::new(),
        cwd: Some(cwd.to_path_buf()),
        owner: None,
    };
    let mut events = Vec::new();
    drive_turn(resolve_binary(&codex), &codex, &request, |event| events.push(event)).await;
    events
}

#[tokio::main]
async fn main() {
    println!("binary: {:?}", resolve_binary(&Codex::default()));
    let cwd = std::env::temp_dir();
    let first = turn("Run the shell command `echo agentdeck` and then say the single word: ready", None, &cwd).await;
    for event in &first {
        println!("1> {event:?}");
    }
    let thread = first.iter().find_map(|event| match event {
        AgentEvent::Done { session_id, .. } => session_id.clone(),
        _ => None,
    });
    println!("thread: {thread:?}");
    let second = turn("What single word did you say last time? Answer with it only.", thread, &cwd).await;
    for event in &second {
        println!("2> {event:?}");
    }
}
