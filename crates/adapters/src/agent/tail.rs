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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use super::interrupt;
use super::stall::Watchdog;
use super::warm::{park, Warm};
use super::{AgentEvent, Provider};

/// Where the tail's events go: the chat, the turn id of the answer, the event.
pub type EventSink = Arc<dyn Fn(&str, &str, AgentEvent) + Send + Sync>;

static SINK: OnceLock<EventSink> = OnceLock::new();
static TAILS: LazyLock<Mutex<HashMap<String, Tail>>> = LazyLock::new(Default::default);
static SERIAL: AtomicU64 = AtomicU64::new(0);

/// A tail with no background work open and nothing to read for this long gives the process back to the pool (which ends
/// it after its own idle time): nothing is going to speak.
const QUIET: Duration = Duration::from_secs(20 * 60);
/// With background work open the CLI may speak much later (a build, a long sub-agent). Giving the process up would end it
/// and, with it, the job it is waiting for (`Warm` kills its process tree when dropped), so the tail waits for as long as a
/// work day. The next message of the chat still takes the process back at once.
const QUIET_WITH_TASKS: Duration = Duration::from_secs(8 * 60 * 60);
/// How long a new message waits for an answer the CLI is giving on its own before that answer is interrupted: it is a
/// report on finished work, so it is short, and the user is waiting.
const HANDOVER: Duration = if cfg!(test) { Duration::from_millis(300) } else { Duration::from_secs(15) };
/// The longest `stop` can take: the wait, the interrupt, the CLI's time to obey, and a little for the bookkeeping.
const HANDOVER_MAX: Duration = Duration::from_secs(15 + 30 + 5);
/// Events held while waiting for the first one that is part of an answer (token counts come before the text).
const HELD: usize = 64;

/// At most this many tails read a process at once (each holds a CLI). A turn that ends with background work open while this
/// many are running parks its process instead, as it did before tails existed.
const MAX_TAILS: usize = 8;

struct Tail {
    /// The provider of the process, so an account switch of one agent ends only its tails.
    agent: &'static str,
    /// Set when the process must end with the tail (an account switch, a reload), not go back to the pool.
    discard: Arc<AtomicBool>,
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

/// How many tails are reading a process now.
pub fn count() -> usize {
    TAILS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).len()
}

/// Whether a tail reads this chat's process now.
pub(super) fn holds(chat: &str) -> bool {
    TAILS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).contains_key(chat)
}

/// Ends every tail, or the tails of one agent, together with the process each one reads: after an account switch a CLI
/// that goes on running would keep the sign-in it started with.
pub(super) fn discard(agent: Option<&str>) {
    let gone: Vec<Tail> = {
        let mut tails = TAILS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let chats: Vec<String> = tails
            .iter()
            .filter(|(_, tail)| agent.is_none_or(|agent| tail.agent == agent))
            .map(|(chat, _)| chat.clone())
            .collect();
        chats.into_iter().filter_map(|chat| tails.remove(&chat)).collect()
    };
    for tail in gone {
        tail.discard.store(true, Ordering::Relaxed);
        let _ = tail.stop.send(());
    }
}

/// Stops the answer in progress of a chat's tail (an interrupt). False when the chat has none.
pub fn interrupt(chat: &str) -> bool {
    TAILS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(chat)
        .is_some_and(|tail| tail.interrupt.send(()).is_ok())
}

/// Takes the process back from the chat's tail, if it has one: waits (`HANDOVER`) for an answer in progress, which is then
/// interrupted, and the process is parked. Returns `false` when the caller's own stop came first: the caller is being
/// replaced and must not wait. The wait is bounded either way, so a message never waits behind a tail for ever.
pub(super) async fn stop(chat: &str, token: &mut interrupt::StopToken) -> bool {
    let tail = TAILS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(chat);
    let Some(Tail { stop, done, .. }) = tail else {
        return true;
    };
    let _ = stop.send(());
    tokio::select! {
        _ = tokio::time::timeout(HANDOVER_MAX, done) => true,
        () = token.stopped() => false,
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


/// Whether a line of the CLI's output says that a sub-agent (not a shell left running) is still open. A delegated worker
/// waits for its sub-agents, whose answer it is asked for, but not for a dev server: that never ends.
pub(super) fn agents_open(line: &str) -> Option<bool> {
    if !line.contains("background_tasks_changed") {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    (value["subtype"] == "background_tasks_changed").then(|| {
        value["tasks"]
            .as_array()
            .is_some_and(|tasks| tasks.iter().any(|task| task["task_type"].as_str().is_some_and(|kind| kind.contains("agent"))))
    })
}
pub(super) fn start(chat: String, warm: Warm, provider: Arc<dyn Provider>) {
    let Some(sink) = SINK.get().cloned() else {
        park(chat, warm);
        return;
    };
    let (stop_tx, stop_rx) = oneshot::channel();
    let (interrupt_tx, interrupt_rx) = mpsc::unbounded_channel();
    let (done_tx, done_rx) = oneshot::channel();
    let discard = Arc::new(AtomicBool::new(false));
    {
        let mut tails = TAILS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if tails.len() >= MAX_TAILS && !tails.contains_key(&chat) {
            drop(tails);
            park(chat, warm);
            return;
        }
        tails.insert(
            chat.clone(),
            Tail { agent: provider.id(), discard: Arc::clone(&discard), stop: stop_tx, interrupt: interrupt_tx, done: done_rx },
        );
    }
    super::trace::note(&chat, "tail", format!("started, background work open={}", warm.open_tasks));
    tokio::spawn(run(chat, warm, provider, sink, stop_rx, interrupt_rx, done_tx, discard));
}

/// A moment that never comes, for a timer branch that is switched off.
fn far() -> Instant {
    Instant::now() + Duration::from_secs(365 * 24 * 60 * 60)
}

/// An event that is part of what the CLI says, not bookkeeping around it.
fn is_content(event: &AgentEvent) -> bool {
    !matches!(event, AgentEvent::Usage { .. } | AgentEvent::Raw { .. } | AgentEvent::Task { .. })
}

enum Fate {
    Park,
    Drop,
}

#[allow(clippy::too_many_arguments)] // the process, its provider, and the four channels the tail is driven through
async fn run(
    chat: String,
    mut warm: Warm,
    provider: Arc<dyn Provider>,
    sink: EventSink,
    mut stop: oneshot::Receiver<()>,
    mut interrupts: mpsc::UnboundedReceiver<()>,
    done: oneshot::Sender<()>,
    discard: Arc<AtomicBool>,
) {
    // The turn id of the answer in progress; empty while the CLI is silent.
    let mut turn = String::new();
    let mut held: Vec<AgentEvent> = Vec::new();
    let mut stopping = false;
    // The CLI was asked to stop its answer (by the user, or because a message is waiting for the process).
    let mut asked = false;
    // A message is waiting for the process while the CLI answers: the moment that answer is interrupted, and the moment
    // the process is given up if it does not obey.
    let mut interrupt_at: Option<Instant> = None;
    let mut kill_at: Option<Instant> = None;
    let mut failure: Option<String> = None;
    let mut dog = Watchdog::new(provider.stall_limit());
    let fate = 'read: loop {
        let wait = if !turn.is_empty() {
            provider.stall_limit()
        } else if warm.open_tasks {
            QUIET_WITH_TASKS
        } else {
            QUIET
        };
        let ask = tokio::select! {
            next = warm.lines.next_segment() => {
                let Ok(Some(bytes)) = next else {
                    failure = Some("agent ended without a result".to_owned());
                    break 'read Fate::Drop;
                };
                dog.touch();
                Some(bytes)
            }
            _ = &mut stop, if !stopping => {
                stopping = true;
                if discard.load(Ordering::Relaxed) {
                    break 'read Fate::Drop;
                }
                if turn.is_empty() {
                    break 'read Fate::Park;
                }
                interrupt_at = Some(Instant::now() + HANDOVER);
                None
            }
            Some(()) = interrupts.recv(), if !turn.is_empty() && !asked => {
                interrupt_at = Some(Instant::now());
                None
            }
            () = tokio::time::sleep_until(interrupt_at.unwrap_or_else(far)), if interrupt_at.is_some() && !asked => {
                let line = format!("{}\n", interrupt::claude_interrupt_line("agentdeck-stop-auto"));
                if warm.stdin.write_all(line.as_bytes()).await.is_ok() && warm.stdin.flush().await.is_ok() {
                    asked = true;
                    kill_at = Some(Instant::now() + interrupt::INTERRUPT_DEADLINE);
                    None
                } else {
                    failure = Some("could not interrupt the agent".to_owned());
                    break 'read Fate::Drop;
                }
            }
            () = tokio::time::sleep_until(kill_at.unwrap_or_else(far)), if kill_at.is_some() => {
                break 'read Fate::Drop;
            }
            () = tokio::time::sleep(wait) => {
                if turn.is_empty() {
                    break 'read Fate::Park;
                }
                failure = Some(dog.message());
                break 'read Fate::Drop;
            }
        };
        let Some(bytes) = ask else {
            continue;
        };
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
                    super::trace::note(&chat, "tail", format!("the CLI went on by itself: {turn}"));
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
            interrupt_at = None;
            kill_at = None;
            if stopping || !warm.open_tasks {
                break 'read Fate::Park;
            }
        }
    };
    // A reply that was open when the process was given up must be closed, or the chat shows it working for ever.
    if matches!(fate, Fate::Drop) && !turn.is_empty() {
        let closing = if asked || stopping {
            interrupt::killed(warm.conversation.as_deref())
        } else {
            AgentEvent::Error { message: failure.clone().unwrap_or_else(|| "agent ended without a result".to_owned()) }
        };
        sink(&chat, &turn, closing);
    }
    super::trace::note(
        &chat,
        "tail",
        format!(
            "ended: {} (answer open={}, stopping={stopping}, asked={asked}){}",
            if matches!(fate, Fate::Park) { "process parked" } else { "process dropped, its children end with it" },
            !turn.is_empty(),
            failure.as_deref().map(|why| format!(" why: {why}")).unwrap_or_default()
        ),
    );
    match fate {
        Fate::Park if !discard.load(Ordering::Relaxed) => park(chat.clone(), warm),
        Fate::Park | Fate::Drop => drop(warm),
    }
    TAILS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&chat);
    let _ = done.send(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::interrupt::StopToken;
    use crate::agent::warm::{drive_warm_turn_with, holds, settings_key};
    use crate::agent::{claude::Claude, TurnRequest};

    type Heard = Vec<(String, String, AgentEvent)>;

    /// What every test's sink heard (the sink is set once per process, so the tests share it and filter by chat).
    static HEARD: LazyLock<Mutex<Heard>> = LazyLock::new(Default::default);

    fn heard_of(chat: &str) -> Vec<(String, AgentEvent)> {
        HEARD
            .lock()
            .unwrap()
            .iter()
            .filter(|(from, _, _)| from == chat)
            .map(|(_, turn, event)| (turn.clone(), event.clone()))
            .collect()
    }

    fn listen() {
        set_sink(Arc::new(|chat, turn, event| HEARD.lock().unwrap().push((chat.to_owned(), turn.to_owned(), event))));
    }

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

    fn script(name: &str, body: &str) -> (std::path::PathBuf, Fake) {
        let path = std::env::temp_dir().join(format!("agentdeck-tail-{name}-{}.sh", std::process::id()));
        std::fs::write(&path, body).unwrap();
        let script = path.to_string_lossy().replace('\\', "/");
        (path, Fake { script })
    }

    fn request(chat: &str) -> TurnRequest {
        TurnRequest {
            prompt: "go".into(),
            session_id: Some("s1".into()),
            resume: false,
            model: None,
            permission_mode: None,
            effort: None,
            allowed_tools: Vec::new(),
            cwd: None,
            owner: Some(chat.into()),
            api_key: None,
        }
    }

    const FIRST_RESULT: &str = r#"read first
echo '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"a"}]}'
echo '{"type":"result","subtype":"success","session_id":"s1","result":"started"}'
sleep 1
echo '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"report"}}}'
"#;

    const FINAL_RESULT: &str = r#"echo '{"type":"result","subtype":"success","session_id":"s1","result":"report"}'
read second
"#;

    const OBEY_INTERRUPT: &str = r#"read interrupt
echo '{"type":"result","subtype":"error_during_execution","is_error":true,"session_id":"s1","result":""}'
read more
"#;

    async fn first_turn(provider: &Fake, chat: &str) {
        let mut own = Vec::new();
        let ran = drive_warm_turn_with("sh".into(), provider, &request(chat), StopToken::never(), |event| own.push(event)).await;
        assert!(ran);
        assert!(matches!(own.last(), Some(AgentEvent::Done { .. })), "the turn ends at its first result, holding no lock");
    }

    async fn until(mut done: impl FnMut() -> bool) -> bool {
        for _ in 0..100 {
            if done() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        done()
    }

    #[tokio::test]
    async fn what_the_cli_says_after_its_turn_becomes_a_reply_of_its_own() {
        listen();
        let (path, provider) = script("report", &format!("{FIRST_RESULT}{FINAL_RESULT}"));
        first_turn(&provider, "chat-tail").await;
        let ended = until(|| heard_of("chat-tail").iter().any(|(_, event)| matches!(event, AgentEvent::Done { .. }))).await;
        let _ = std::fs::remove_file(&path);
        assert!(ended, "no answer of its own arrived: {:?}", heard_of("chat-tail"));
        let told = heard_of("chat-tail");
        assert!(matches!(told.first(), Some((_, AgentEvent::AutoTurn { .. }))), "{told:?}");
        assert!(told.iter().any(|(_, event)| matches!(event, AgentEvent::Delta { text } if text == "report")), "{told:?}");
        assert!(matches!(told.last(), Some((_, AgentEvent::Done { .. }))), "{told:?}");
        assert!(told.iter().all(|(turn, _)| turn.starts_with("auto-")));

        // The answer over, the process goes back to the pool for the chat's next message.
        let key = settings_key("faketail", &provider.args(&request("chat-tail")), &[]);
        assert!(until(|| holds("chat-tail", &key, Some("s1"), false)).await, "the process was not parked after the answer");
        // Only this chat's process: `reset` would end the other tests' tails too (they share the agent id).
        let _ = crate::agent::warm::take("chat-tail", &key, None);
    }

    #[tokio::test]
    async fn a_message_does_not_wait_for_ever_behind_an_answer_the_cli_gives_on_its_own() {
        listen();
        // The CLI starts an answer and carries on until it is interrupted, as a long report would.
        let (path, provider) = script("handover", &format!("{FIRST_RESULT}{OBEY_INTERRUPT}"));
        first_turn(&provider, "chat-handover").await;
        assert!(until(|| heard_of("chat-handover").iter().any(|(_, event)| matches!(event, AgentEvent::Delta { .. }))).await, "the answer never started");

        let started = std::time::Instant::now();
        let mut token = StopToken::never();
        assert!(stop("chat-handover", &mut token).await, "the process came back");
        let _ = std::fs::remove_file(&path);
        assert!(started.elapsed() < Duration::from_secs(10), "the message waited {:?}", started.elapsed());

        let told = heard_of("chat-handover");
        assert!(
            matches!(told.last(), Some((_, AgentEvent::Done { .. }))),
            "the interrupted answer is closed as a stopped one, not left working: {told:?}"
        );
        let key = settings_key("faketail", &provider.args(&request("chat-handover")), &[]);
        assert!(holds("chat-handover", &key, Some("s1"), false), "the process is parked for the message");
        // Only this chat's process: `reset` would end the other tests' tails too (they share the agent id).
        let _ = crate::agent::warm::take("chat-handover", &key, None);
    }

    #[tokio::test]
    async fn a_replaced_turn_stops_waiting_for_the_tail() {
        listen();
        // This CLI never obeys an interrupt: only the caller's own stop can end the wait.
        let (path, provider) = script("deaf", &format!("{FIRST_RESULT}sleep 3\n"));
        first_turn(&provider, "chat-deaf").await;
        assert!(until(|| heard_of("chat-deaf").iter().any(|(_, event)| matches!(event, AgentEvent::Delta { .. }))).await);

        let (handle, mut token) = crate::agent::interrupt::StopHandle::new();
        let waiter = tokio::spawn(async move { stop("chat-deaf", &mut token).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        handle.stop();
        let answered = tokio::time::timeout(Duration::from_secs(5), waiter).await;
        let _ = std::fs::remove_file(&path);
        assert!(matches!(answered, Ok(Ok(false))), "the wait did not end with the caller's stop: {answered:?}");
        // Only this chat's process: `reset` would end the other tests' tails too (they share the agent id).
        let key = settings_key("faketail", &provider.args(&request("chat-deaf")), &[]);
        let _ = crate::agent::warm::take("chat-deaf", &key, None);
    }
}
