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
//! turn: stopping the turn ends it, and what it started, like a per-turn run.

use std::collections::HashMap;
use std::ffi::OsString;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Split};
use tokio::process::{Child, ChildStdin, ChildStdout};

use super::resolve::augmented_path;
use super::runner::{command_for, drain_stderr, kill_tree};
use super::{AgentEvent, Provider, TurnRequest};

/// An idle process ends after this long, so a chat left open does not keep a CLI (and its memory) for ever.
const IDLE: Duration = Duration::from_secs(10 * 60);
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
    /// Started ahead of the chat's first message (`prewarm`) and not used yet: it takes a turn that names no
    /// conversation, which is how a new chat's first turn arrives.
    pub(super) fresh: bool,
    /// Which parking this is, so an idle timer only ends the process it was set for.
    serial: u64,
}

impl Drop for Warm {
    /// A process is dropped when its turn is stopped or replaced, when it fails, or when it idles out or is evicted:
    /// in every case the CLI goes, with everything it started.
    fn drop(&mut self) {
        if let Some(pid) = self.child.id() {
            kill_tree(pid);
        }
    }
}

/// The idle processes, by chat.
static POOL: LazyLock<Mutex<HashMap<String, Warm>>> = LazyLock::new(Default::default);
static SERIAL: AtomicU64 = AtomicU64::new(0);

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
    let mut warm = pool().remove(chat)?;
    fits(&mut warm, key, conversation).then_some(warm)
}

/// The chat's idle process with these settings, whatever conversation it holds (Codex's server opens another thread
/// on the same process). One with other settings is ended.
pub(super) fn take_server(chat: &str, key: &str) -> Option<Warm> {
    let mut warm = pool().remove(chat)?;
    (warm.key == key && matches!(warm.child.try_wait(), Ok(None))).then_some(warm)
}

/// Whether the chat already keeps a process that would take this turn, or (`any_conversation`) any process with these
/// settings. Nothing is taken or ended.
pub(super) fn holds(chat: &str, key: &str, conversation: Option<&str>, any_conversation: bool) -> bool {
    let mut pool = pool();
    let Some(warm) = pool.get_mut(chat) else { return false };
    if any_conversation {
        warm.key == key && matches!(warm.child.try_wait(), Ok(None))
    } else {
        fits(warm, key, conversation)
    }
}

/// Puts a process back for the chat's next turn, and sets its idle timer.
pub(super) fn park(chat: String, mut warm: Warm) {
    let serial = SERIAL.fetch_add(1, Ordering::Relaxed) + 1;
    warm.serial = serial;
    let mut evicted = Vec::new();
    {
        let mut pool = pool();
        evicted.extend(pool.insert(chat.clone(), warm));
        while pool.len() > MAX_IDLE {
            let Some(oldest) = pool.iter().min_by_key(|(_, warm)| warm.serial).map(|(chat, _)| chat.clone()) else {
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
        fresh: false,
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

/// Reads the turn's lines until its result. The process stays up afterwards.
async fn read_turn(warm: &mut Warm, provider: &dyn Provider, emit: &mut impl FnMut(AgentEvent)) -> Ending {
    let Warm { lines, child, .. } = warm;
    let mut deadline: Option<tokio::time::Instant> = None;
    loop {
        let next = match deadline {
            Some(at) => tokio::time::timeout_at(at, lines.next_segment()).await.unwrap_or(Ok(None)),
            None => tokio::select! {
                next = lines.next_segment() => next,
                _ = child.wait() => {
                    deadline = Some(tokio::time::Instant::now() + EXIT_GRACE);
                    continue;
                }
            },
        };
        let Ok(Some(bytes)) = next else { return Ending::Closed };
        let text = String::from_utf8_lossy(&bytes);
        let line = text.strip_suffix('\r').unwrap_or(&text);
        let mut ending = None;
        for event in provider.parse_line(line) {
            match &event {
                AgentEvent::Done { session_id, .. } => ending = Some(Ending::Done(session_id.clone())),
                AgentEvent::Error { .. } => ending = Some(Ending::Failed),
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
    let env = provider.environment(req);
    let key = settings_key(provider.id(), &args, &env);
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
pub async fn drive_warm_turn(binary: OsString, provider: &dyn Provider, req: &TurnRequest, mut emit: impl FnMut(AgentEvent)) -> bool {
    if provider.id() == "codex" {
        return super::codex_server::drive(binary, provider, req, emit).await;
    }
    let (Some(args), Some(chat)) = (provider.warm_args(req), req.owner.clone()) else {
        return false;
    };
    if let Err(message) = provider.prepare(req) {
        emit(AgentEvent::Error { message });
        return true;
    }
    let env = provider.environment(req);
    let key = settings_key(provider.id(), &args, &env);
    let line = format!("{}\n", provider.warm_message(&req.prompt));

    let mut warm = match take(&chat, &key, req.session_id.as_deref()) {
        Some(mut warm) => {
            warm.fresh = false;
            warm
        }
        None => match start(binary.clone(), provider, &args, &env, req, &key) {
            Ok(warm) => warm,
            Err(message) => {
                emit(AgentEvent::Error { message });
                return true;
            }
        },
    };
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

    match read_turn(&mut warm, provider, &mut emit).await {
        Ending::Done(conversation) => {
            if conversation.is_some() {
                warm.conversation = conversation;
            }
            park(chat, warm);
        }
        Ending::Failed => drop(warm),
        Ending::Closed => {
            let stderr = warm
                .stderr
                .lock()
                .map(|kept| String::from_utf8_lossy(&kept).trim().to_owned())
                .unwrap_or_default();
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
    fn the_settings_key_holds_the_environment_and_the_agent() {
        let plain = settings_key("claude", &args(&["-p"]), &[]);
        let keyed = settings_key("claude", &args(&["-p"]), &[("ANTHROPIC_API_KEY".to_owned(), "k".to_owned())]);
        assert_ne!(plain, keyed, "another key is another account");
        assert!(settings_key("agy", &args(&["-p"]), &[]).starts_with(&format!("agy{SEP}")));
    }
}
