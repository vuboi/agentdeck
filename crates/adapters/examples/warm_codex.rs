//! Real check of Codex kept open through `codex app-server`: a server started ahead of the first message (prewarm), then
//! turns timed from send to result, with how many text pieces streamed and when the first one came. The third turn
//! changes the effort and the fourth runs in auto mode: both must keep the conversation. Costs a few cents.
//! Run: `cargo run -p ag-adapters --example warm_codex`

use std::time::{Duration, Instant};

use ag_adapters::agent::codex::Codex;
use ag_adapters::agent::runner::resolve_binary;
use ag_adapters::agent::warm::{drive_warm_turn, prewarm, reset};
use ag_adapters::agent::{AgentEvent, TurnRequest};

fn request(prompt: &str, thread: Option<String>, effort: &str, mode: &str) -> TurnRequest {
    TurnRequest {
        prompt: prompt.into(),
        resume: thread.is_some(),
        session_id: thread,
        model: Some("gpt-6-luna".into()),
        permission_mode: Some(mode.into()),
        effort: Some(effort.into()),
        allowed_tools: Vec::new(),
        cwd: Some(std::env::temp_dir()),
        owner: Some("warm-check-codex".into()),
        api_key: None,
    }
}

#[tokio::main]
async fn main() {
    let codex = Codex::default();
    let binary = resolve_binary(&codex);

    let started = Instant::now();
    prewarm(binary.clone(), &codex, &request("", None, "low", "acceptEdits")).await;
    println!("prewarm: {}ms", started.elapsed().as_millis());
    // The user is still typing.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let turns = [
        ("Remember the word kiwi. Reply only: ok", "low", "acceptEdits"),
        ("Which word did I ask you to remember? One word.", "low", "acceptEdits"),
        ("Count from 1 to 5, one per line.", "medium", "acceptEdits"),
        ("Which word did I ask you to remember? One word.", "low", "auto"),
    ];
    let mut thread: Option<String> = None;
    for (index, (prompt, effort, mode)) in turns.iter().enumerate() {
        let started = Instant::now();
        let (mut pieces, mut first, mut text, mut end) = (0, None, String::new(), String::new());
        let warm = drive_warm_turn(binary.clone(), &codex, &request(prompt, thread.clone(), effort, mode), |event| match event {
            AgentEvent::Delta { text: piece } => {
                pieces += 1;
                first.get_or_insert(started.elapsed().as_millis());
                text.push_str(&piece);
            }
            AgentEvent::Done { session_id, .. } => thread = session_id.or(thread.clone()),
            AgentEvent::Error { message } => end = format!(" ERROR {message}"),
            _ => {}
        })
        .await;
        assert!(warm);
        println!(
            "turn {} ({effort}, {mode}): {}ms, first text {}ms, {pieces} pieces {text:?}{end}",
            index + 1,
            started.elapsed().as_millis(),
            first.unwrap_or(0)
        );
    }
    reset(None);
}
