//! Real end-to-end check of the OpenCode turn runner: one conversation, two turns, then a tool call.
//! Uses a free model, so it costs nothing. Run: `cargo run -p ag-adapters --example opencode_turn`
//!
//! Turn 1 remembers a word, turn 2 asks for it back (which checks `--session`), and turn 3 runs a shell command
//! (which checks that a finished tool line becomes a call and a result). Needs `opencode` installed and signed in.

use ag_adapters::agent::opencode::OpenCode;
use ag_adapters::agent::runner::{drive_turn, resolve_binary};
use ag_adapters::agent::{AgentEvent, TurnRequest};

async fn turn(
    provider: &OpenCode,
    prompt: &str,
    session: Option<String>,
    cwd: &std::path::Path,
) -> Vec<AgentEvent> {
    let request = TurnRequest {
        prompt: prompt.into(),
        resume: session.is_some(),
        session_id: session,
        model: Some("opencode/big-pickle".into()),
        permission_mode: Some("auto".into()),
        effort: None,
        allowed_tools: Vec::new(),
        cwd: Some(cwd.to_path_buf()),
        owner: None,
        api_key: None,
    };
    let mut events = Vec::new();
    drive_turn(resolve_binary(provider), provider, &request, |event| {
        events.push(event)
    })
    .await;
    events
}

fn summary(events: &[AgentEvent]) -> String {
    let count =
        |matching: fn(&AgentEvent) -> bool| events.iter().filter(|event| matching(event)).count();
    let text: String = events
        .iter()
        .filter_map(|event| {
            if let AgentEvent::Delta { text } = event {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect();
    let calls = count(|event| matches!(event, AgentEvent::ToolCall { .. }));
    let results = count(|event| matches!(event, AgentEvent::ToolResult { .. }));
    let usage = events
        .iter()
        .any(|event| matches!(event, AgentEvent::Usage { .. }));
    let session = events.iter().find_map(|event| match event {
        AgentEvent::Done { session_id, .. } => session_id.clone(),
        _ => None,
    });
    format!(
        "answer {:?}, {calls} call(s), {results} result(s), usage {usage}, session {session:?}",
        text.trim()
    )
}

#[tokio::main]
async fn main() {
    let cwd = std::env::temp_dir().join("agentdeck-opencode-check");
    std::fs::create_dir_all(&cwd).expect("temp folder");
    let provider = OpenCode::default();
    println!("binary: {:?}", resolve_binary(&provider));
    let models = ag_adapters::models::opencode(&ag_adapters::agent::opencode::models_listing(
        resolve_binary(&provider),
    ));
    println!(
        "models: {} (first: {:?})",
        models.len(),
        models.first().map(|model| (&model.value, &model.label))
    );

    let first = turn(
        &provider,
        "Remember the word kiwi. Reply only: ok",
        None,
        &cwd,
    )
    .await;
    println!("turn 1: {}", summary(&first));
    let Some(session) = first.iter().find_map(|event| match event {
        AgentEvent::Done { session_id, .. } => session_id.clone(),
        _ => None,
    }) else {
        panic!("no session id to resume: {first:?}");
    };

    println!(
        "turn 2: {}",
        summary(
            &turn(
                &provider,
                "What word did I ask you to remember? One word.",
                Some(session.clone()),
                &cwd
            )
            .await
        )
    );
    println!(
        "turn 3: {}",
        summary(
            &turn(
                &provider,
                "Run the shell command 'echo hi' and then say done.",
                Some(session.clone()),
                &cwd
            )
            .await
        )
    );
    // Shell characters are the reason the prompt goes in on stdin: as an argument to the Windows `.cmd` shim they
    // would be re-parsed as a second command.
    println!(
        "turn 4: {}",
        summary(
            &turn(
                &provider,
                "Repeat this line exactly: a & b | c > d",
                Some(session),
                &cwd
            )
            .await
        )
    );
}
