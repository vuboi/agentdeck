//! Real check of Antigravity kept open and started ahead of the first message: the CLI is started (prewarm), the
//! user "types" for a few seconds, then the first message and a follow-up are timed from send to result.
//! Run: `cargo run -p ag-adapters --example warm_agy`

use std::time::{Duration, Instant};

use ag_adapters::agent::antigravity::Antigravity;
use ag_adapters::agent::runner::resolve_binary;
use ag_adapters::agent::warm::{drive_warm_turn, prewarm, reset};
use ag_adapters::agent::{AgentEvent, TurnRequest};

fn request(prompt: &str, conversation: Option<String>) -> TurnRequest {
    TurnRequest {
        prompt: prompt.into(),
        resume: conversation.is_some(),
        session_id: conversation,
        model: Some("gemini-3.8-flash-low".into()),
        permission_mode: None,
        effort: None,
        allowed_tools: Vec::new(),
        cwd: Some(std::env::temp_dir()),
        owner: Some("warm-check-agy".into()),
        api_key: None,
    }
}

#[tokio::main]
async fn main() {
    let agy = Antigravity::default();
    let binary = resolve_binary(&agy);
    let started = Instant::now();
    prewarm(binary.clone(), &agy, &request("", None)).await;
    println!("prewarm returned after {}ms", started.elapsed().as_millis());
    tokio::time::sleep(Duration::from_secs(8)).await;
    let mut conversation: Option<String> = None;
    for prompt in ["Reply only: ok", "Reply only: done"] {
        let sent = Instant::now();
        let (mut first, mut text) = (None, String::new());
        drive_warm_turn(binary.clone(), &agy, &request(prompt, conversation.clone()), |event| match event {
            AgentEvent::Delta { text: piece } => {
                first.get_or_insert(sent.elapsed().as_millis());
                text.push_str(&piece);
            }
            AgentEvent::Done { session_id, .. } => conversation = session_id.or(conversation.clone()),
            AgentEvent::Error { message } => text = format!("ERROR {message}"),
            _ => {}
        })
        .await;
        println!("turn {prompt:?}: {}ms, first text {}ms {text:?}", sent.elapsed().as_millis(), first.unwrap_or(0));
    }
    reset(None);
}
