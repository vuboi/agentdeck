//! Real check of the approval flow: a write is refused, the user "approves", the next turn writes the file.
//! Costs a few cents (Haiku). Run: `cargo run -p ag-adapters --example agent_permission`

use ag_adapters::agent::claude::Claude;
use ag_adapters::agent::runner::{drive_turn, resolve_binary};
use ag_adapters::agent::{AgentEvent, TurnRequest};

async fn turn(prompt: &str, id: &str, resume: bool, allowed: Vec<String>, cwd: &std::path::Path) -> Vec<AgentEvent> {
    let request = TurnRequest {
        prompt: prompt.into(),
        session_id: Some(id.into()),
        resume,
        model: Some("claude-haiku-5-5".into()),
        permission_mode: None,
        allowed_tools: allowed,
        cwd: Some(cwd.to_path_buf()),
    };
    let mut events = Vec::new();
    drive_turn(resolve_binary(&Claude), &Claude, &request, |event| events.push(event)).await;
    events
}

#[tokio::main]
async fn main() {
    let cwd = std::env::temp_dir().join("agentdeck-permission-check");
    let _ = std::fs::remove_dir_all(&cwd);
    std::fs::create_dir_all(&cwd).expect("temp folder");
    let id = format!("{:08x}-2222-4333-8444-{:012x}", std::process::id(), std::process::id());

    let first = turn("Create a file named ok.txt containing hi, using the Write tool.", &id, false, Vec::new(), &cwd).await;
    let asked: Vec<_> = first
        .iter()
        .filter_map(|event| match event {
            AgentEvent::PermissionDenied { requests } => Some(requests.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    println!("turn 1: permission requests = {asked:?}");
    println!("file exists after turn 1: {}", cwd.join("ok.txt").exists());

    let allow: Vec<String> = asked.iter().map(|request| request.allow.clone()).collect();
    let second = turn("Permission granted. Please continue with what you were doing.", &id, true, allow, &cwd).await;
    println!("turn 2 ended with: {:?}", second.last().map(|event| matches!(event, AgentEvent::Done { .. })));
    println!("file exists after turn 2: {}", cwd.join("ok.txt").exists());
}
