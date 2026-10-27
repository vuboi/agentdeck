//! Real end-to-end check of kept (warm) CLI processes: three turns of one chat on Claude, then on Antigravity, each
//! timed from send to its result. The first turn starts the CLI; the next ones reuse it. Costs a few cents.
//! Run: `cargo run -p ag-adapters --example warm_turn`

use std::time::Instant;

use ag_adapters::agent::antigravity::Antigravity;
use ag_adapters::agent::claude::Claude;
use ag_adapters::agent::codex::Codex;
use ag_adapters::agent::runner::resolve_binary;
use ag_adapters::agent::warm::{drive_warm_turn, reset};
use ag_adapters::agent::{AgentEvent, Provider, TurnRequest};

async fn turn(provider: &dyn Provider, prompt: &str, session: Option<String>, resume: bool, model: &str) -> (u128, Option<String>, String) {
    let request = TurnRequest {
        prompt: prompt.into(),
        session_id: session,
        resume,
        model: Some(model.into()),
        permission_mode: None,
        effort: None,
        allowed_tools: Vec::new(),
        cwd: Some(std::env::temp_dir()),
        owner: Some(format!("warm-check-{}", provider.id())),
        api_key: None,
    };
    let started = Instant::now();
    let mut end = (None, String::new());
    let mut said = String::new();
    let warm = drive_warm_turn(resolve_binary(provider), provider, &request, |event| match event {
        AgentEvent::Delta { text } => said.push_str(&text),
        AgentEvent::Done { session_id, text, .. } => end = (session_id, if text.is_empty() { said.clone() } else { text }),
        AgentEvent::Error { message } => end = (None, format!("ERROR {message}")),
        _ => {}
    })
    .await;
    assert!(warm, "{} keeps its process", provider.id());
    (started.elapsed().as_millis(), end.0, end.1)
}

#[tokio::main]
async fn main() {
    let id = format!("{:08x}-2222-4333-8444-{:012x}", std::process::id(), std::process::id());
    let prompts = ["Remember the word kiwi. Reply only: ok", "Which word did I ask you to remember? One word.", "Reply only: done"];

    let mut session = Some(id);
    for (index, prompt) in prompts.iter().enumerate() {
        let (ms, reported, text) = turn(&Claude, prompt, session.clone(), index > 0, "claude-haiku-5-5").await;
        println!("claude turn {}: {ms}ms {text:?}", index + 1);
        session = reported.or(session);
    }

    // agy names its conversation itself: the first turn has no id, the next ones resume the one it reported.
    let mut conversation: Option<String> = None;
    for (index, prompt) in prompts.iter().enumerate() {
        let (ms, reported, text) = turn(&Antigravity::default(), prompt, conversation.clone(), conversation.is_some(), "gemini-3.8-flash-low").await;
        println!("agy turn {}: {ms}ms {text:?}", index + 1);
        conversation = reported.or(conversation);
    }
    let mut thread: Option<String> = None;
    for (index, prompt) in prompts.iter().enumerate() {
        let (ms, reported, text) = turn(&Codex::default(), prompt, thread.clone(), thread.is_some(), "gpt-6-luna").await;
        println!("codex turn {}: {ms}ms {text:?}", index + 1);
        thread = reported.or(thread);
    }
    reset(None);
}
