//! Real check of Claude kept open and started ahead of the first message: the CLI is started (prewarm), the
//! user "types" for a few seconds, then the first message and a follow-up are timed from send to result.
//! Run: `cargo run -p ag-adapters --example warm_agy`

use std::time::{Duration, Instant};

use ag_adapters::agent::claude::Claude;
use ag_adapters::agent::runner::resolve_binary;
use ag_adapters::agent::warm::{drive_warm_turn, prewarm, reset};
use ag_adapters::agent::{AgentEvent, TurnRequest};

const HAIKU: &str = "claude-haiku-5-5";
const SONNET: &str = "claude-sonnet-5-5";

fn request(prompt: &str, model: &str, conversation: Option<String>, resume: bool) -> TurnRequest {
    TurnRequest {
        prompt: prompt.into(),
        resume,
        session_id: conversation,
        model: Some(model.into()),
        permission_mode: None,
        effort: None,
        allowed_tools: Vec::new(),
        cwd: Some(std::env::temp_dir()),
        owner: Some("warm-check-claude".into()),
        api_key: None,
    }
}

#[tokio::main]
async fn main() {
    let agy = Claude;
    let id = format!("{:08x}-3333-4444-8555-{:012x}", std::process::id(), std::process::id());
    let binary = resolve_binary(&agy);
    let started = Instant::now();
    prewarm(binary.clone(), &agy, &request("", HAIKU, Some(id.clone()), false)).await;
    println!("prewarm returned after {}ms", started.elapsed().as_millis());
    tokio::time::sleep(Duration::from_secs(8)).await;
    let mut conversation: Option<String> = Some(id.clone());
    let mut resume = false;
    // The last turn changes the model: it goes to the same process with `set_model` (no new start), in the same conversation.
    for (prompt, model) in [("Reply only: ok", HAIKU), ("Reply only: done", HAIKU), ("Reply only the name of the model you are.", SONNET)] {
        let sent = Instant::now();
        let (mut first, mut text) = (None, String::new());
        drive_warm_turn(binary.clone(), &agy, &request(prompt, model, conversation.clone(), resume), |event| match event {
            AgentEvent::Delta { text: piece } => {
                first.get_or_insert(sent.elapsed().as_millis());
                text.push_str(&piece);
            }
            AgentEvent::Done { session_id, .. } => conversation = session_id.or(conversation.clone()),
            AgentEvent::Error { message } => text = format!("ERROR {message}"),
            _ => {}
        })
        .await;
        resume = true;
        println!("turn {prompt:?} on {model}: {}ms, first text {}ms {text:?}", sent.elapsed().as_millis(), first.unwrap_or(0));
    }
    reset(None);
}
