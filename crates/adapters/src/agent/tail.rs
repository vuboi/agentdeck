//! What a kept CLI does after its turn, while background work it started is still open.
//!
//! Claude ends a turn (`result`) as soon as the model answers, even when a sub-agent or a `run_in_background` shell is still
//! running. When that work ends the CLI starts a turn of its own, with no message from the user. Nobody reads the process
//! between turns, so that answer was lost and the next message read the old output. A turn that waited for the work
//! instead held the chat's lock for as long as a dev server ran.
//!
//! So the turn ends at its `result`, as it always did, and the process goes to a tail: a task that keeps reading it and
//! sends what the CLI says on its own to the chat as a new reply (`AutoTurn`). The next message of the chat stops the
//! tail first (`stop`), which waits for an answer in progress and then parks the process for that message. A stop of the
//! chat interrupts the tail's answer (`interrupt`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};

use super::interrupt;
use super::stall::Watchdog;
use super::warm::{park, Warm};
use super::{AgentEvent, Provider};

/// Where the tail's events go: the chat, the turn id of the answer, the event.
pub type EventSink = Arc<dyn Fn(&str, &str, AgentEvent) + Send + Sync>;

static SINK: OnceLock<EventSink> = OnceLock::new();
static TAILS: LazyLock<Mutex<HashMap<String, Tail>>> = LazyLock::new(Default::default);
static SERIAL: AtomicU64 = AtomicU64::new(0);

/// A tail that has nothing to read for this long gives the process back to the pool (which ends it after its own idle time).
const QUIET: Duration = Duration::from_secs(20 * 60);
/// How long a new turn waits for a tail's answer in progress before it starts on its own.
const HANDOVER: Duration = Duration::from_secs(10 * 60);
/// Events held while waiting for the first one that is part of an answer (token counts come before the text).
const HELD: usize = 64;

struct Tail {
    stop: oneshot::Sender<()>,
    interrupt: mpsc::UnboundedSender<()>,
    done: oneshot::Receiver<()>,
}

/// Registers where tail events are sent. Called once by the app; without it no tail starts.
pub fn set_sink(sink: EventSink) {
    let _ = SINK.set(sink);
}

pub(super) fn available() -> bool {
    SINK.get().is_some()
}

/// Stops the answer in progress of a chat's tail (an interrupt). False when the chat has none.
pub fn interrupt(chat: &str) -> bool {
    TAILS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(chat)
        .is_some_and(|tail| tail.interrupt.send(()).is_ok())
}

/// Takes the process back from the chat's tail, if it has one: waits for an answer in progress, then the process is parked.
pub(super) async fn stop(chat: &str) {
    let tail = TAILS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(chat);
    if let Some(Tail { stop, done, .. }) = tail {
        let _ = stop.send(());
        let _ = tokio::time::timeout(HANDOVER, done).await;
    }
}

/// Whether a line of the CLI's output says which background tasks are open, and if any are.
pub(super) fn background_open(line: &str) -> Option<bool> {
    if !line.contains("background_tasks_changed") {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    (value["subtype"] == "background_tasks_changed").then(|| value["tasks"].as_array().is_some_and(|tasks| !tasks.is_empty()))
}

pub(super) fn start(chat: String, warm: Warm, provider: Arc<dyn Provider>) {
    let Some(sink) = SINK.get().cloned() else {
        park(chat, warm);
        return;
    };
    let (stop_tx, stop_rx) = oneshot::channel();
    let (interrupt_tx, interrupt_rx) = mpsc::unbounded_channel();
    let (done_tx, done_rx) = oneshot::channel();
    TAILS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(chat.clone(), Tail { stop: stop_tx, interrupt: interrupt_tx, done: done_rx });
    tokio::spawn(run(chat, warm, provider, sink, stop_rx, interrupt_rx, done_tx));
}

/// An event that is part of what the CLI says, not bookkeeping around it.
fn is_content(event: &AgentEvent) -> bool {
    !matches!(event, AgentEvent::Usage { .. } | AgentEvent::Raw { .. } | AgentEvent::Task { .. })
}

enum Fate {
    Park,
    Drop,
}

async fn run(
    chat: String,
    mut warm: Warm,
    provider: Arc<dyn Provider>,
    sink: EventSink,
    mut stop: oneshot::Receiver<()>,
    mut interrupts: mpsc::UnboundedReceiver<()>,
    done: oneshot::Sender<()>,
) {
    // The turn id of the answer in progress; empty while the CLI is silent.
    let mut turn = String::new();
    let mut held: Vec<AgentEvent> = Vec::new();
    let mut stopping = false;
    let mut asked = false;
    let mut dog = Watchdog::new(provider.stall_limit());
    let fate = 'read: loop {
        let wait = if turn.is_empty() { QUIET } else { provider.stall_limit() };
        let next = tokio::select! {
            next = warm.lines.next_segment() => next,
            _ = &mut stop, if !stopping => {
                stopping = true;
                if turn.is_empty() {
                    break 'read Fate::Park;
                }
                continue;
            }
            Some(()) = interrupts.recv(), if !turn.is_empty() && !asked => {
                let line = format!("{}\n", interrupt::claude_interrupt_line("agentdeck-stop-auto"));
                if warm.stdin.write_all(line.as_bytes()).await.is_ok() && warm.stdin.flush().await.is_ok() {
                    asked = true;
                    continue;
                }
                break 'read Fate::Drop;
            }
            _ = tokio::time::sleep(wait) => {
                if turn.is_empty() {
                    break 'read Fate::Park;
                }
                sink(&chat, &turn, AgentEvent::Error { message: dog.message() });
                break 'read Fate::Drop;
            }
        };
        let Ok(Some(bytes)) = next else {
            if !turn.is_empty() {
                sink(&chat, &turn, AgentEvent::Error { message: "agent ended without a result".to_owned() });
            }
            break 'read Fate::Drop;
        };
        dog.touch();
        let text = String::from_utf8_lossy(&bytes);
        let line = text.strip_suffix('\r').unwrap_or(&text);
        if let Some(open) = background_open(line) {
            warm.open_tasks = open;
        }
        let mut ended = false;
        for event in provider.parse_line(line) {
            let event = if asked { interrupt::stopped(event, warm.conversation.as_deref()) } else { event };
            let terminal = matches!(event, AgentEvent::Done { .. } | AgentEvent::Error { .. });
            if turn.is_empty() {
                if is_content(&event) {
                    turn = format!("auto-{}", SERIAL.fetch_add(1, Ordering::Relaxed) + 1);
                    sink(&chat, &turn, AgentEvent::AutoTurn { reason: "background work finished".to_owned() });
                    for earlier in held.drain(..) {
                        sink(&chat, &turn, earlier);
                    }
                    sink(&chat, &turn, event);
                } else if held.len() < HELD {
                    held.push(event);
                }
            } else {
                sink(&chat, &turn, event);
            }
            if terminal && !turn.is_empty() {
                ended = true;
            }
        }
        if ended {
            turn.clear();
            held.clear();
            asked = false;
            if stopping || !warm.open_tasks {
                break 'read Fate::Park;
            }
        }
    };
    match fate {
        Fate::Park => park(chat.clone(), warm),
        Fate::Drop => drop(warm),
    }
    TAILS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&chat);
    let _ = done.send(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::warm::{drive_warm_turn_with, holds, settings_key};
    use crate::agent::{claude::Claude, TurnRequest};
    use crate::agent::interrupt::StopToken;

    /// A Claude-like provider whose "CLI" is `sh script`.
    struct Fake {
        script: String,
    }
    impl Provider for Fake {
        fn id(&self) -> &'static str {
            "faketail"
        }
        fn binary(&self) -> &'static str {
            "sh"
        }
        fn candidates(&self) -> Vec<std::path::PathBuf> {
            Vec::new()
        }
        fn args(&self, _req: &TurnRequest) -> Vec<String> {
            vec![self.script.clone()]
        }
        fn warm_args(&self, req: &TurnRequest) -> Option<Vec<String>> {
            Some(self.args(req))
        }
        fn warm_message(&self, prompt: &str) -> String {
            Claude.warm_message(prompt)
        }
        fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
            Claude.parse_line(line)
        }
        fn tail_provider(&self) -> Option<Arc<dyn Provider>> {
            Some(Arc::new(Fake { script: self.script.clone() }))
        }
    }

    #[tokio::test]
    async fn what_the_cli_says_after_its_turn_becomes_a_reply_of_its_own() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("agentdeck-tail-{}.sh", std::process::id()));
        let body = r#"read first
echo '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"a"}]}'
echo '{"type":"result","subtype":"success","session_id":"s1","result":"started"}'
sleep 1
echo '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"report"}}}'
echo '{"type":"result","subtype":"success","session_id":"s1","result":"report"}'
read second
"#;
        std::fs::write(&path, body).unwrap();
        let script = path.to_string_lossy().replace('\\', "/");
        let provider = Fake { script };

        let heard: Arc<Mutex<Vec<(String, String, AgentEvent)>>> = Arc::default();
        let sink_heard = Arc::clone(&heard);
        set_sink(Arc::new(move |chat, turn, event| sink_heard.lock().unwrap().push((chat.to_owned(), turn.to_owned(), event))));

        let req = TurnRequest {
            prompt: "go".into(),
            session_id: Some("s1".into()),
            resume: false,
            model: None,
            permission_mode: None,
            effort: None,
            allowed_tools: Vec::new(),
            cwd: None,
            owner: Some("chat-tail".into()),
            api_key: None,
        };
        let mut own = Vec::new();
        let ran = drive_warm_turn_with("sh".into(), &provider, &req, StopToken::never(), |event| own.push(event)).await;
        assert!(ran);
        assert!(matches!(own.last(), Some(AgentEvent::Done { .. })), "the turn ends at its first result, holding no lock");

        let mut told = Vec::new();
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            told = heard.lock().unwrap().clone();
            if told.iter().any(|(_, _, event)| matches!(event, AgentEvent::Done { .. })) {
                break;
            }
        }
        let _ = std::fs::remove_file(&path);
        let kinds: Vec<&AgentEvent> = told.iter().map(|(_, _, event)| event).collect();
        assert!(matches!(kinds.first(), Some(AgentEvent::AutoTurn { .. })), "{kinds:?}");
        assert!(kinds.iter().any(|event| matches!(event, AgentEvent::Delta { text } if text == "report")), "{kinds:?}");
        assert!(matches!(kinds.last(), Some(AgentEvent::Done { .. })), "{kinds:?}");
        assert!(told.iter().all(|(chat, turn, _)| chat == "chat-tail" && turn.starts_with("auto-")));

        // The answer over, the process goes back to the pool for the chat's next message.
        let key = settings_key("faketail", &provider.args(&req), &[]);
        let mut parked = false;
        for _ in 0..50 {
            if holds("chat-tail", &key, Some("s1"), false) {
                parked = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(parked, "the process was not parked after the answer");
        crate::agent::warm::reset(Some("faketail"));
    }
}
