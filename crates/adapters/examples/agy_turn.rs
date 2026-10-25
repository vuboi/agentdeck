//! Runs real Antigravity turns through the provider: `cargo run -p ag-adapters --example agy_turn -- "<prompt>" [model]`.
//!
//! Turn 1 runs the prompt. If agy refused a command, turn 2 repeats the prompt with that command approved (which
//! writes its rule into agy's settings.json); otherwise turn 2 asks what was asked, which checks resume.
//! Needs `agy` signed in.

use ag_adapters::agent::antigravity::Antigravity;
use ag_adapters::agent::runner::{drive_turn, resolve_binary};
use ag_adapters::agent::{AgentEvent, TurnRequest};

fn request(
    prompt: &str,
    model: Option<String>,
    session: Option<String>,
    allowed: Vec<String>,
) -> TurnRequest {
    TurnRequest {
        prompt: prompt.into(),
        resume: session.is_some(),
        session_id: session,
        model,
        permission_mode: Some("acceptEdits".into()),
        effort: None,
        allowed_tools: allowed,
        cwd: std::env::current_dir().ok(),
        owner: None,
    }
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let prompt = args
        .next()
        .unwrap_or_else(|| "List the files in this folder, then say done.".into());
    let model = args.next();
    let provider = Antigravity::default();
    let mut conversation = None;
    let mut approvals: Vec<String> = Vec::new();
    let mut next = prompt.clone();
    for turn in 1..=2 {
        println!("--- turn {turn}");
        let req = request(
            &next,
            model.clone(),
            conversation.clone(),
            approvals.clone(),
        );
        let mut asked = Vec::new();
        drive_turn(resolve_binary(&provider), &provider, &req, |event| {
            match &event {
                AgentEvent::Done {
                    session_id: Some(id),
                    ..
                } => conversation = Some(id.clone()),
                AgentEvent::PermissionDenied { requests } => {
                    asked.extend(requests.iter().map(|request| request.allow.clone()))
                }
                _ => {}
            }
            println!("{event:?}");
        })
        .await;
        next = if asked.is_empty() {
            "What did I just ask you? One short sentence.".to_string()
        } else {
            approvals = asked;
            prompt.clone()
        };
    }
}
