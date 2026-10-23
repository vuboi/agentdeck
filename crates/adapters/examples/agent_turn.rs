//! Real end-to-end check of the Claude turn runner: two turns of one conversation, then a look at disk.
//! Costs a few cents (Haiku). Run: `cargo run -p ag-adapters --example agent_turn`

use ag_adapters::agent::claude::Claude;
use ag_adapters::agent::runner::{drive_turn, resolve_binary};
use ag_adapters::agent::{AgentEvent, TurnRequest};

async fn turn(prompt: &str, id: &str, resume: bool, cwd: &std::path::Path) -> Vec<AgentEvent> {
    let request = TurnRequest {
        prompt: prompt.into(),
        session_id: Some(id.into()),
        resume,
        model: Some("claude-haiku-5-5".into()),
        permission_mode: None,
        effort: None,
        allowed_tools: Vec::new(),
        cwd: Some(cwd.to_path_buf()),
        owner: None,
    };
    let mut events = Vec::new();
    drive_turn(resolve_binary(&Claude), &Claude, &request, |event| {
        events.push(event)
    })
    .await;
    events
}

fn summary(events: &[AgentEvent]) -> String {
    let deltas: usize = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::Delta { .. }))
        .count();
    match events.last() {
        Some(AgentEvent::Done {
            text, session_id, ..
        }) => {
            format!("{deltas} deltas, done: {text:?}, session {session_id:?}")
        }
        other => format!("{deltas} deltas, ended with {other:?}"),
    }
}

#[tokio::main]
async fn main() {
    let cwd = std::env::temp_dir().join("agentdeck-turn-check");
    std::fs::create_dir_all(&cwd).expect("temp folder");
    let id = format!(
        "{:08x}-1111-4222-8333-{:012x}",
        std::process::id(),
        std::process::id()
    );
    println!("binary: {:?}", resolve_binary(&Claude));

    println!(
        "turn 1: {}",
        summary(&turn("Remember this word: kiwi. Reply only: ok", &id, false, &cwd).await)
    );
    println!(
        "turn 2: {}",
        summary(
            &turn(
                "What word did I ask you to remember? One word.",
                &id,
                true,
                &cwd
            )
            .await
        )
    );

    // Both turns must be one conversation file, not two.
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .expect("home");
    let found: Vec<_> = ag_adapters::history::list_all(&ag_adapters::history::Roots::from_home(
        std::path::Path::new(&home),
    ))
    .into_iter()
    .filter(|session| session.id.contains(&id))
    .collect();
    println!("sessions on disk with this id: {}", found.len());
}
