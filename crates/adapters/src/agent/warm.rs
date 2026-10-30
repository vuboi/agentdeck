//! Agent CLIs kept open between the turns of a chat.
//!
//! Starting `claude -p` or `agy -p` loads the user's settings, plugins, skills, MCP servers and hooks before the model is
//! asked anything: seconds on every message, even "hi". Both CLIs can instead read one message per JSON line on stdin and
//! keep the conversation in the same process (`--input-format stream-json`). The first message of a chat starts the CLI as
//! before; the next ones go to the process that is already up.
//!
//! A process is reused only by the same chat, with the same settings (model, effort, permissions, allowed tools, folder,
//! key) and the same conversation. Anything else starts a new one, which resumes the conversation by id as a per-turn run
//! does. An idle process ends after `IDLE`, and at most `MAX_IDLE` are kept. A process running a turn belongs to that
//! turn: a stop interrupts the turn (`interrupt.rs`) and the process goes back to the pool once the CLI reports it; only a CLI
//! that stays silent for `INTERRUPT_DEADLINE`, or has no interrupt message, is ended with what it started.

use std::collections::HashMap;
use std::ffi::OsString;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Split};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use super::interrupt::{self, InterruptMode, Interrupter, Signal, StopToken};
use super::stall::Watchdog;
use super::resolve::augmented_path;
use super::runner::{command_for, drain_stderr, kill_tree};
use super::{AgentEvent, Provider, TurnRequest};

/// An idle process ends after this long, so a chat left open does not keep a CLI (and its memory) for ever.
const IDLE: Duration = Duration::from_secs(20 * 60);
/// At most this many idle processes are kept; the one parked longest ago goes first.
const MAX_IDLE: usize = 4;
/// Output that may still come after the CLI exited mid-turn.
pub(super) const EXIT_GRACE: Duration = Duration::from_secs(2);
/// The flags that name the conversation. They are left out of the settings key and compared on their own.
const CONVERSATION_FLAGS: [&str; 3] = ["--resume", "--session-id", "--conversation"];
/// Separates the parts of a settings key (a character no flag holds).
const SEP: char = '\u{1f}';

pub(super) struct Warm {
    /// The CLI and its settings, without the conversation (see `settings_key`).
    pub(super) key: String,
    /// The conversation the process holds: the id it was started with, or the one its last turn reported.
    pub(super) conversation: Option<String>,
    pub(super) child: Child,
    pub(super) stdin: ChildStdin,
    pub(super) lines: Split<BufReader<ChildStdout>>,
    pub(super) stderr: Arc<Mutex<Vec<u8>>>,
    /// The last request id sent to a process that speaks JSON-RPC (Codex's app server).
    pub(super) rpc: u64,
    /// The model the CLI runs now: the one it started with, or the one its last `set_model` named (`live_model`).
    pub(super) model: Option<String>,
    /// Started ahead of the chat's first message (`prewarm`) and not used yet: it takes a turn that names no
    /// conversation, which is how a new chat's first turn arrives.
    pub(super) fresh: bool,
    /// Background work the CLI started (a sub-agent, a shell left running) is still open: it may go on by itself when
    /// that ends (see `tail.rs`).
    pub(super) open_tasks: bool,
    /// Which parking this is, so an idle timer only ends the process it was set for.
    serial: u64,
}

impl Drop for Warm {
    /// A process is dropped when its turn is stopped or replaced, when it fails, or when it idles out or is evicted:
    /// in every case the CLI goes, with everything it started.
    fn drop(&mut self) {
        if let Some(pid) = self.child.id() {
            super::runner::release_pid(pid);
            kill_tree(pid);
        }
    }
}

/// One lock per chat: a conversation has one writer, so a turn waits for the previous turn of the chat to end (a stopped
/// one is still being interrupted) and a server started ahead (`prewarm`) does not race the turn that needs it. The turn
/// holds its chat's lock until it ends.
static LOCKS: LazyLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = LazyLock::new(Default::default);

pub(super) fn lock_of(chat: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut locks = LOCKS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // A lock nobody holds or waits for (only the map owns it) belongs to a turn that is over: it goes, so the map does
    // not keep one entry for every chat and worker the app ever ran.
    locks.retain(|key, lock| key == chat || Arc::strong_count(lock) > 1);
    Arc::clone(locks.entry(chat.to_owned()).or_default())
}

/// Where a message written while a turn runs is handed to that turn, by chat. Only a turn on a kept Claude process
/// registers here: Claude's stream-json input takes a message at any time.
static INJECT: LazyLock<Mutex<HashMap<String, UnboundedSender<String>>>> = LazyLock::new(Default::default);

/// Hands a message to the turn running for `chat`, which writes it to the CLI at once. The CLI reads it at its next step
/// (between two tool calls) in the same turn, or, when the turn is only writing text, as the turn that follows. False
/// when the chat has no such turn: the caller keeps the message for after the turn.
pub fn inject(chat: &str, prompt: &str) -> bool {
    let senders = INJECT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    senders.get(chat).is_some_and(|sender| sender.send(prompt.to_owned()).is_ok())
}

/// Registers a turn to take injected messages, and takes the registration back when the turn ends.
pub struct InjectSlot(String);

impl InjectSlot {
    /// Opens the chat's inbox for as long as the slot lives. A provider that runs its own loop (a custom chat provider)
    /// reads it between its steps; a message `inject` reports as taken must be read.
    pub fn open(chat: &str) -> (InjectSlot, UnboundedReceiver<String>) {
        let (sender, receiver) = unbounded_channel();
        INJECT.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(chat.to_owned(), sender);
        (InjectSlot(chat.to_owned()), receiver)
    }
}

impl Drop for InjectSlot {
    fn drop(&mut self) {
        INJECT.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&self.0);
    }
}

/// Whether a line is the CLI reading back a message of the user (`--replay-user-messages`), which is how a turn knows a
/// message written to it was taken.
fn is_replay(line: &str) -> bool {
    line.contains("\"isReplay\":true")
        && serde_json::from_str::<serde_json::Value>(line)
            .is_ok_and(|value| value["type"] == "user" && value["message"]["content"].is_string())
}

/// The idle processes, by chat and agent (`slot`): a process started ahead for the agent a chat may fall back to does not
/// take the place of the one its own agent keeps.
static POOL: LazyLock<Mutex<HashMap<String, Warm>>> = LazyLock::new(Default::default);
static SERIAL: AtomicU64 = AtomicU64::new(0);

/// Where the idle process of `chat` with the settings `key` is kept.
fn slot(chat: &str, key: &str) -> String {
    format!("{chat}{SEP}{}", key.split(SEP).next().unwrap_or_default())
}

/// How many idle processes are kept.
pub fn pool_len() -> usize {
    pool().len()
}

fn pool() -> MutexGuard<'static, HashMap<String, Warm>> {
    POOL.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The CLI and everything it was started with, except the conversation, as one string.
pub(super) fn settings_key(agent: &str, args: &[String], env: &[(String, String)]) -> String {
    let mut parts = vec![agent.to_owned()];
    let mut skip = false;
    for arg in args {
        if skip {
            skip = false;
        } else if CONVERSATION_FLAGS.contains(&arg.as_str()) {
            skip = true;
        } else {
            parts.push(arg.clone());
        }
    }
    parts.extend(env.iter().map(|(name, value)| format!("{name}={value}")));
    parts.join(&SEP.to_string())
}

/// The key of a kept process. A CLI that can switch its model live (`live_model`) is not restarted for another model: the
/// model is left out, and the switch is sent to the process (see `drive_warm_turn_with`).
fn key_for(provider: &dyn Provider, args: &[String], env: &[(String, String)]) -> String {
    if !provider.live_model() {
        return settings_key(provider.id(), args, env);
    }
    let mut kept = Vec::with_capacity(args.len());
    let mut model = false;
    for arg in args {
        if model {
            model = false;
        } else if arg == "--model" {
            model = true;
        } else {
            kept.push(arg.clone());
        }
    }
    settings_key(provider.id(), &kept, env)
}

/// Ends the kept processes of one agent, or of all: after an account switch the next message must start with the new
/// sign-in, which a running CLI would not pick up.
pub fn reset(agent: Option<&str>) {
    let gone: Vec<Warm> = {
        let mut pool = pool();
        let chats: Vec<String> = pool
            .iter()
            .filter(|(_, warm)| match agent {
                Some(agent) => warm.key.starts_with(&format!("{agent}{SEP}")),
                None => true,
            })
            .map(|(chat, _)| chat.clone())
            .collect();
        chats.into_iter().filter_map(|chat| pool.remove(&chat)).collect()
    };
    drop(gone);
    // A process a tail is reading keeps its sign-in too.
    super::tail::discard(agent);
}

/// Whether a kept process can take a turn: same settings, still running, and the turn's conversation (a turn that
/// names none takes a process started ahead of it).
fn fits(warm: &mut Warm, key: &str, conversation: Option<&str>) -> bool {
    let same = match conversation {
        Some(_) => warm.conversation.as_deref() == conversation,
        None => warm.fresh,
    };
    warm.key == key && same && matches!(warm.child.try_wait(), Ok(None))
}

/// The chat's idle process, if it can take this turn (see `fits`). One that cannot is ended.
pub(super) fn take(chat: &str, key: &str, conversation: Option<&str>) -> Option<Warm> {
    let mut warm = pool().remove(&slot(chat, key))?;
    fits(&mut warm, key, conversation).then_some(warm)
}

/// The chat's idle process with these settings, whatever conversation it holds (Codex's server opens another thread
/// on the same process). One with other settings is ended.
pub(super) fn take_server(chat: &str, key: &str) -> Option<Warm> {
    let mut warm = pool().remove(&slot(chat, key))?;
    (warm.key == key && matches!(warm.child.try_wait(), Ok(None))).then_some(warm)
}

/// Whether the chat already keeps a process that would take this turn, or (`any_conversation`) any process with these
/// settings. Nothing is taken or ended.
pub(super) fn holds(chat: &str, key: &str, conversation: Option<&str>, any_conversation: bool) -> bool {
    let mut pool = pool();
    let Some(warm) = pool.get_mut(&slot(chat, key)) else { return false };
    if any_conversation {
        warm.key == key && matches!(warm.child.try_wait(), Ok(None))
    } else {
        fits(warm, key, conversation)
    }
}

/// Puts a process back for the chat's next turn, and sets its idle timer.
pub(super) fn park(chat: String, mut warm: Warm) {
    let chat = slot(&chat, &warm.key);
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed) + 1;
    warm.serial = serial;
    let mut evicted = Vec::new();
    {
        let mut pool = pool();
        evicted.extend(pool.insert(chat.clone(), warm));
        while pool.len() > MAX_IDLE {
            // A process started ahead and never used goes first: it was a guess, the others were somebody's chat.
            let Some(oldest) = pool.iter().min_by_key(|(_, warm)| (!warm.fresh, warm.serial)).map(|(chat, _)| chat.clone()) else {
                break;
            };
            evicted.extend(pool.remove(&oldest));
        }
    }
    // Ended outside the lock.
    drop(evicted);
    tokio::spawn(async move {
        tokio::time::sleep(IDLE).await;
        let idle = {
            let mut pool = pool();
            if pool.get(&chat).is_some_and(|warm| warm.serial == serial) {
                pool.remove(&chat)
            } else {
                None
            }
        };
        drop(idle);
    });
}

/// Starts the CLI as a per-turn run would (same environment, folder, PATH and process group), reading JSON lines.
pub(super) fn start(binary: OsString, provider: &dyn Provider, args: &[String], env: &[(String, String)], req: &TurnRequest, key: &str) -> Result<Warm, String> {
    let mut command = command_for(binary, args);
    command
        .envs(env.iter().cloned())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(path) = augmented_path() {
        command.env("PATH", path);
    }
    if let Some(dir) = req.cwd.as_deref().filter(|dir| dir.is_dir()) {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start {}: {error}. Check that its CLI is installed in Settings → Connectors.", provider.binary()))?;
    super::runner::spawned(&child);
    if let (Some(pid), Some(chat)) = (child.id(), req.owner.as_deref()) {
        super::runner::claim_pid(pid, chat);
    }
    let (Some(stdin), Some(stdout), Some(stderr)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else {
        return Err(format!("could not talk to {}", provider.binary()));
    };
    let kept = Arc::new(Mutex::new(Vec::new()));
    tokio::spawn(drain_stderr(stderr, Arc::clone(&kept)));
    Ok(Warm {
        key: key.to_owned(),
        conversation: req.session_id.clone(),
        child,
        stdin,
        lines: BufReader::new(stdout).split(b'\n'),
        stderr: kept,
        rpc: 0,
        model: req.model.clone(),
        fresh: false,
        open_tasks: false,
        serial: 0,
    })
}

pub(super) async fn send(warm: &mut Warm, line: &str) -> std::io::Result<()> {
    warm.stdin.write_all(line.as_bytes()).await?;
    warm.stdin.flush().await
}

/// How a turn on a kept process ended.
enum Ending {
    /// The turn finished; the process can take the next one. Carries the conversation id it reported.
    Done(Option<String>),
    /// The turn failed: the process is not reused (it may be in a state the next turn should not inherit).
    Failed,
    /// The CLI went away without finishing the turn.
    Closed,
}

/// Reads the turn's lines until its result. The process stays up afterwards. A stop (`stop`) sends the CLI its interrupt
/// message and keeps reading: the result that follows ends the turn as a stopped one and the process is kept. A CLI
/// without an interrupt message, or one silent for the deadline, is given up (`Failed`: the caller ends it).
async fn read_turn(warm: &mut Warm, provider: &dyn Provider, stop: StopToken, mut inject: Option<UnboundedReceiver<String>>, chat: &str, emit: &mut impl FnMut(AgentEvent)) -> Ending {
    let conversation = warm.conversation.clone();
    let began = std::time::Instant::now();
    let mut first_line = true;
    let Warm { lines, child, stdin, open_tasks, .. } = warm;
    let mut stopper = Interrupter::new(stop);
    let mut requests = 0u32;
    let mut deadline: Option<tokio::time::Instant> = None;
    // Counts from the start of the turn, not from when the process was parked: an idle process says nothing.
    let mut dog = Watchdog::new(provider.stall_limit());
    // Messages written to the CLI that it has not read back yet (the turn's own prompt counts), and how many were
    // injected: a result that arrives while an injected one is unread ends only a part of the work, the CLI runs on.
    let mut unread = u32::from(inject.is_some());
    let mut injected = 0u32;
    loop {
        let next = match deadline {
            Some(at) => tokio::time::timeout_at(at, lines.next_segment()).await.unwrap_or(Ok(None)),
            None => tokio::select! {
                next = lines.next_segment() => next,
                _ = child.wait() => {
                    deadline = Some(tokio::time::Instant::now() + EXIT_GRACE);
                    continue;
                }
                _ = tokio::time::sleep_until(dog.deadline()) => {
                    emit(AgentEvent::Error { message: dog.message() });
                    return Ending::Failed;
                }
                Some(prompt) = async {
                    match inject.as_mut() {
                        Some(receiver) => receiver.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    let line = format!("{}\n", provider.warm_message(&prompt));
                    if stdin.write_all(line.as_bytes()).await.is_ok() && stdin.flush().await.is_ok() {
                        unread += 1;
                        injected += 1;
                    }
                    continue;
                }
                signal = stopper.signal() => {
                    let asked = signal == Signal::Ask && provider.interrupt_mode() == InterruptMode::StreamJson;
                    if asked {
                        requests += 1;
                        let line = format!("{}\n", interrupt::claude_interrupt_line(&format!("agentdeck-stop-{requests}")));
                        if stdin.write_all(line.as_bytes()).await.is_ok() && stdin.flush().await.is_ok() {
                            continue;
                        }
                    }
                    emit(interrupt::killed(conversation.as_deref()));
                    return Ending::Failed;
                }
            },
        };
        let Ok(Some(bytes)) = next else { return Ending::Closed };
        dog.touch();
        if std::mem::take(&mut first_line) {
            super::trace::note(chat, "first line", format!("{} ms after the message was sent", began.elapsed().as_millis()));
        }
        let text = String::from_utf8_lossy(&bytes);
        let line = text.strip_suffix('\r').unwrap_or(&text);
        let mut ending = None;
        if unread > 0 && is_replay(line) {
            unread -= 1;
        }
        // Background work (a sub-agent, a shell left running) does not hold the turn: it ends at its result, and what the
        // CLI does by itself afterwards is read by the tail (see `tail.rs`). The watchdog just allows for the silence.
        if let Some(open) = super::tail::background_open(line) {
            *open_tasks = open;
            dog.set_background(open);
        }
        for event in provider.parse_line(line) {
            dog.note(&event);
            let event = if stopper.asked() { interrupt::stopped(event, conversation.as_deref()) } else { event };
            match &event {
                // The end of a part of the work while an injected message is still unread: the CLI goes on with it as the
                // next turn, so this result is not the end and is not shown as one.
                AgentEvent::Done { .. } if injected > 0 && unread > 0 => continue,
                AgentEvent::Done { session_id, .. } => ending = Some(Ending::Done(session_id.clone().or_else(|| conversation.clone()))),
                // A stream-json CLI reports a failed turn as its own result and waits for the next message: the process
                // is kept. Any other CLI may be left in a state the next turn should not inherit, so it is not.
                AgentEvent::Error { .. } => {
                    ending = Some(if provider.interrupt_mode() == InterruptMode::StreamJson {
                        Ending::Done(conversation.clone())
                    } else {
                        Ending::Failed
                    })
                }
                _ => {}
            }
            emit(event);
        }
        if let Some(ending) = ending {
            return ending;
        }
    }
}

/// Starts the chat's CLI before its message is sent (the user is typing), so the first turn does not wait for the CLI
/// to load its settings, plugins, MCP servers and hooks. The request is the one the message will be sent with (its
/// prompt is not used). Does nothing for an agent that does not keep processes, or when the chat already has one.
pub async fn prewarm(binary: OsString, provider: &dyn Provider, req: &TurnRequest) {
    if provider.id() == "codex" {
        return super::codex_server::prewarm(binary, provider, req).await;
    }
    let (Some(args), Some(chat)) = (provider.warm_args(req), req.owner.clone()) else {
        return;
    };
    if provider.prepare(req).is_err() {
        return;
    }
    // The chat's lock, as a turn takes it (and `codex_server::prewarm`): a process is not started beside one a turn is using.
    let lock = lock_of(&chat);
    let _turn = lock.lock().await;
    if super::tail::holds(&chat) {
        return;
    }
    let env = provider.environment(req);
    let key = key_for(provider, &args, &env);
    if holds(&chat, &key, req.session_id.as_deref(), false) {
        return;
    }
    if let Ok(mut warm) = start(binary, provider, &args, &env, req, &key) {
        warm.fresh = true;
        park(chat, warm);
    }
}

/// Runs one turn on a kept process (starting it if the chat has none that fits). Returns `false`, having started and
/// emitted nothing, when this provider or request does not keep processes: the caller then runs a per-turn process.
pub async fn drive_warm_turn(binary: OsString, provider: &dyn Provider, req: &TurnRequest, emit: impl FnMut(AgentEvent)) -> bool {
    drive_warm_turn_with(binary, provider, req, StopToken::never(), emit).await
}

/// [`drive_warm_turn`] that can be stopped: when `stop` fires the CLI is interrupted, the turn ends with its result and
/// the process stays in the pool (see `interrupt.rs`).
pub async fn drive_warm_turn_with(binary: OsString, provider: &dyn Provider, req: &TurnRequest, stop: StopToken, mut emit: impl FnMut(AgentEvent)) -> bool {
    if provider.id() == "codex" {
        return super::codex_server::drive(binary, provider, req, stop, emit).await;
    }
    let (Some(args), Some(chat)) = (provider.warm_args(req), req.owner.clone()) else {
        return false;
    };
    if let Err(message) = provider.prepare(req) {
        emit(AgentEvent::Error { message });
        return true;
    }
    let lock = lock_of(&chat);
    let asked_at = std::time::Instant::now();
    let _turn = lock.lock().await;
    super::trace::note(&chat, "chat lock", format!("{} waited {} ms", provider.id(), asked_at.elapsed().as_millis()));
    // A process that was left reading what the CLI does on its own comes back first, after an answer in progress.
    if !super::tail::stop(&chat, &mut stop.clone()).await {
        // Stopped (replaced) while waiting for the process: the turn ends here, with nothing written.
        emit(interrupt::killed(req.session_id.as_deref()));
        return true;
    }
    // Only Claude reads a message at any time and tells when it did (`--replay-user-messages`).
    let (_slot, inbox) = if provider.id() == "claude" {
        let (slot, receiver) = InjectSlot::open(&chat);
        (Some(slot), Some(receiver))
    } else {
        (None, None)
    };
    let env = provider.environment(req);
    let key = key_for(provider, &args, &env);
    let line = format!("{}\n", provider.warm_message(&super::turn_prompt(provider, req)));

    let mut warm = match take(&chat, &key, req.session_id.as_deref()) {
        Some(mut warm) => {
            warm.fresh = false;
            super::trace::note(&chat, "process", format!("kept one takes the turn pid={:?}", warm.child.id()));
            warm
        }
        None => match start(binary.clone(), provider, &args, &env, req, &key) {
            Ok(warm) => {
                super::trace::note(&chat, "process", format!("started pid={:?} resume={}", warm.child.id(), req.resume));
                warm
            }
            Err(message) => {
                super::trace::note(&chat, "process", format!("could not start: {message}"));
                emit(AgentEvent::Error { message });
                return true;
            }
        },
    };
    // A model change on a live CLI is sent to it, not met with a new process: the conversation goes on in the same one.
    if provider.live_model() {
        if let Some(model) = req.model.as_deref().filter(|model| warm.model.as_deref() != Some(*model)) {
            warm.rpc += 1;
            let line = format!("{}\n", provider.model_switch_line(&format!("agentdeck-model-{}", warm.rpc), model));
            if send(&mut warm, &line).await.is_ok() {
                warm.model = Some(model.to_owned());
            }
        }
    }
    // A kept process may have ended while it waited: it is replaced once.
    if send(&mut warm, &line).await.is_err() {
        drop(warm);
        warm = match start(binary, provider, &args, &env, req, &key) {
            Ok(warm) => warm,
            Err(message) => {
                emit(AgentEvent::Error { message });
                return true;
            }
        };
        if let Err(error) = send(&mut warm, &line).await {
            emit(AgentEvent::Error { message: format!("could not send the message to {}: {error}", provider.binary()) });
            return true;
        }
    }

    let began = std::time::Instant::now();
    match read_turn(&mut warm, provider, stop, inbox, &chat, &mut emit).await {
        Ending::Done(conversation) => {
            if conversation.is_some() {
                warm.conversation = conversation;
            }
            let tail = provider.tail_provider().filter(|_| warm.open_tasks && super::tail::available());
            super::trace::note(&chat, "turn read", format!("done after {:.1}s background_open={} -> {}", began.elapsed().as_secs_f32(), warm.open_tasks, if tail.is_some() { "tail" } else { "park" }));
            match tail {
                Some(reader) => super::tail::start(chat, warm, reader),
                None => park(chat, warm),
            }
        }
        Ending::Failed => {
            super::trace::note(&chat, "turn read", format!("failed after {:.1}s: the process is dropped (its children end with it)", began.elapsed().as_secs_f32()));
            drop(warm)
        }
        Ending::Closed => {
            let stderr = warm
                .stderr
                .lock()
                .map(|kept| String::from_utf8_lossy(&kept).trim().to_owned())
                .unwrap_or_default();
            super::trace::note(&chat, "turn read", format!("the CLI went away after {:.1}s; stderr: {}", began.elapsed().as_secs_f32(), stderr.chars().rev().take(300).collect::<String>().chars().rev().collect::<String>()));
            drop(warm);
            emit(AgentEvent::Error {
                message: if stderr.is_empty() { "agent ended without a result".to_owned() } else { stderr },
            });
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn the_settings_key_leaves_the_conversation_out() {
        let first = settings_key("claude", &args(&["-p", "--session-id", "a", "--model", "m"]), &[]);
        let next = settings_key("claude", &args(&["-p", "--resume", "a", "--model", "m"]), &[]);
        assert_eq!(first, next, "starting and resuming the same conversation is the same process");
        let other_model = settings_key("claude", &args(&["-p", "--resume", "a", "--model", "n"]), &[]);
        assert_ne!(first, other_model);
    }

    #[test]
    fn a_live_model_stays_out_of_the_key_and_other_clis_keep_it() {
        let claude = |model: &str| key_for(&super::super::claude::Claude, &args(&["-p", "--resume", "a", "--model", model]), &[]);
        assert_eq!(claude("m"), claude("n"), "a model change keeps the process");
        let agy = |model: &str| key_for(&super::super::antigravity::Antigravity::default(), &args(&["-p", "--model", model]), &[]);
        assert_ne!(agy("m"), agy("n"), "a CLI without a live switch still starts for another model");
    }

    #[test]
    fn the_settings_key_holds_the_environment_and_the_agent() {
        let plain = settings_key("claude", &args(&["-p"]), &[]);
        let keyed = settings_key("claude", &args(&["-p"]), &[("ANTHROPIC_API_KEY".to_owned(), "k".to_owned())]);
        assert_ne!(plain, keyed, "another key is another account");
        assert!(settings_key("agy", &args(&["-p"]), &[]).starts_with(&format!("agy{SEP}")));
    }
}

#[cfg(all(test, unix))] // the fake CLIs are sh scripts
mod unix_tests {
    use super::*;
    use crate::agent::interrupt::StopHandle;
    use std::os::unix::fs::PermissionsExt;

    /// A Claude-like provider whose "CLI" is a shell script.
    struct Fake;
    impl Provider for Fake {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn binary(&self) -> &'static str {
            "fake"
        }
        fn candidates(&self) -> Vec<std::path::PathBuf> {
            Vec::new()
        }
        fn args(&self, _req: &TurnRequest) -> Vec<String> {
            Vec::new()
        }
        fn warm_args(&self, req: &TurnRequest) -> Option<Vec<String>> {
            Some(self.args(req))
        }
        fn warm_message(&self, prompt: &str) -> String {
            super::super::claude::Claude.warm_message(prompt)
        }
        fn interrupt_mode(&self) -> InterruptMode {
            InterruptMode::StreamJson
        }
        fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
            super::super::claude::Claude.parse_line(line)
        }
    }

    /// Same, but without an interrupt message.
    struct NoInterrupt;
    impl Provider for NoInterrupt {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn binary(&self) -> &'static str {
            "fake"
        }
        fn candidates(&self) -> Vec<std::path::PathBuf> {
            Vec::new()
        }
        fn args(&self, _req: &TurnRequest) -> Vec<String> {
            Vec::new()
        }
        fn warm_args(&self, req: &TurnRequest) -> Option<Vec<String>> {
            Fake.warm_args(req)
        }
        fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
            Fake.parse_line(line)
        }
    }

    fn script(name: &str, body: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("agentdeck-warm-{name}-{}.sh", std::process::id()));
        std::fs::write(&path, format!("#!/bin/sh
{body}
")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn req(chat: &str) -> TurnRequest {
        TurnRequest {
            prompt: "hi".into(),
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

    /// Starts the turn, waits until its first delta came out, then stops it.
    async fn stopped_turn(provider: &'static dyn Provider, path: &std::path::Path, chat: &str) -> Vec<AgentEvent> {
        let (handle, token) = StopHandle::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&events);
        let binary = path.to_path_buf().into_os_string();
        let request = req(chat);
        let turn = tokio::spawn(async move {
            drive_warm_turn_with(binary, provider, &request, token, |event| seen.lock().unwrap().push(event)).await
        });
        for _ in 0..200 {
            if !events.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        handle.stop();
        let ended = tokio::time::timeout(Duration::from_secs(15), turn).await;
        assert!(matches!(ended, Ok(Ok(true))), "the stopped turn did not end");
        let list = events.lock().unwrap().clone();
        list
    }

    const WORKING: &str = r#"read first
echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"half"}}}'"#;

    #[tokio::test]
    async fn a_stop_interrupts_the_turn_and_the_process_stays_warm() {
        // Works on the first message; on the interrupt it reports an error result, as Claude does, and goes on listening.
        let path = script(
            "interrupt",
            &format!("{WORKING}
read second
case \"$second\" in *interrupt*) echo '{{\"type\":\"result\",\"is_error\":true,\"result\":\"interrupted\",\"session_id\":\"s1\"}}';; esac
read third"),
        );
        let events = stopped_turn(&Fake, &path, "chat-interrupt").await;
        let _ = std::fs::remove_file(&path);
        assert_eq!(events.first(), Some(&AgentEvent::Delta { text: "half".into() }));
        assert_eq!(
            events.last(),
            Some(&AgentEvent::Done { session_id: Some("s1".into()), text: String::new(), cost_usd: None }),
            "the interrupted result closes the turn as a stopped one, not as an error"
        );
        let key = settings_key("fake", &[], &[]);
        assert!(holds("chat-interrupt", &key, Some("s1"), false), "the CLI went back to the pool");
        reset(Some("fake"));
    }

    #[tokio::test]
    async fn a_cli_without_an_interrupt_message_is_ended_at_once() {
        let path = script("nointerrupt", &format!("{WORKING}
sleep 30"));
        let events = stopped_turn(&NoInterrupt, &path, "chat-nointerrupt").await;
        let _ = std::fs::remove_file(&path);
        assert!(matches!(events.last(), Some(AgentEvent::Done { .. })));
        let key = settings_key("fake", &[], &[]);
        assert!(!holds("chat-nointerrupt", &key, Some("s1"), true), "a killed CLI is not kept");
    }
}
