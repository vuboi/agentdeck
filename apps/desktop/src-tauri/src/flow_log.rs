//! The debug log (`logs/debug.log`): the whole picture of how the app and its agents moved, for finding out later why it
//! hung, crashed or ran a flow wrong. The audit log only holds calls that finished and the error log only failures, so a
//! turn that never ends, a lock nobody gives or a page that froze left nothing. This one keeps:
//!   - every step of a turn (started, each event the agent sent to the chat in short, ended and why),
//!   - the Rust side's notes (`ag_adapters::agent::trace`: locks, which process took a turn, tails, watchdog),
//!   - the page's notes (`flow-log.ts`: sends, queue, workers, a page that stopped answering),
//!   - a heartbeat every 20 s listing the turns that are running and how long each has been silent,
//!   - panics, with their backtrace.
//!
//! It holds no text the user wrote and none of what the agent answered: streamed text is only counted. Commands the agent
//! ran are cut to a line, as in the audit log. Notes are handed to a writer thread: a caller never waits for the disk.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ag_adapters::agent::{AgentEvent, TaskStatus};
use tauri::{AppHandle, Manager};

/// Past this size the log is moved to `debug.1.log` (one older file kept) and a new one starts.
const ROTATE_BYTES: u64 = 16 * 1024 * 1024;
/// What a read returns: the end of the log.
const READ_BYTES: u64 = 1024 * 1024;
/// Notes waiting for the writer; a burst past this is dropped rather than slowing a turn down.
const QUEUE: usize = 8192;
/// Streamed text is written as one line per this long, with how much came.
const COALESCE: Duration = Duration::from_secs(2);
/// How often the heartbeat lists the running turns.
const HEARTBEAT: Duration = Duration::from_secs(20);

static WRITER: OnceLock<SyncSender<String>> = OnceLock::new();
/// Whether the debug log is written (Settings > Debug). On until the page says otherwise.
static ENABLED: AtomicBool = AtomicBool::new(true);
static PATH: OnceLock<PathBuf> = OnceLock::new();
/// When the page last wrote, and its last line (cut): a page that stops writing has stopped answering.
static PAGE_LAST: Mutex<Option<(Instant, String)>> = Mutex::new(None);
/// Past this long without a page line the heartbeat says the page is silent (a hidden window may slow the page's timers).
const PAGE_SILENT: Duration = Duration::from_secs(45);

/// A turn that is running: what the heartbeat reports of it.
struct Live {
    agent: String,
    started: Instant,
    last_event: Instant,
    last_kind: &'static str,
    events: u64,
    /// Text streamed since the last line written (kind, count, characters, since).
    stream: Option<(&'static str, u64, usize, Instant)>,
}

static LIVE: LazyLock<Mutex<HashMap<(String, String), Live>>> = LazyLock::new(Default::default);

fn live() -> std::sync::MutexGuard<'static, HashMap<(String, String), Live>> {
    LIVE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn log_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|error| error.to_string())?.join("logs");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir.join("debug.log"))
}

/// The time as `2026-10-29T04:12:09.123Z` (UTC), like the page writes it.
fn now() -> String {
    time_at(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default())
}

/// How long the log keeps its lines: older ones are dropped at start-up.
const KEEP: Duration = Duration::from_secs(2 * 24 * 3600);

/// Drops the lines older than `KEEP` from the log and its older file. Each line starts with its time (`"at"`), in the
/// same UTC form, so the lines compare as text. A line that does not say its time is kept.
fn prune_old(path: &Path) {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let cutoff = time_at(since.saturating_sub(KEEP));
    for file in [path.with_file_name("debug.1.log"), path.to_path_buf()] {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let kept: String = text
            .lines()
            .filter(|line| match line.split("\"at\":\"").nth(1).and_then(|rest| rest.split('"').next()) {
                Some(at) => at >= cutoff.as_str(),
                None => true,
            })
            .map(|line| format!("{line}\n"))
            .collect();
        if kept.len() != text.len() {
            let _ = std::fs::write(&file, kept);
        }
    }
}

fn time_at(since: std::time::Duration) -> String {
    let (secs, millis) = (since.as_secs() as i64, since.subsec_millis());
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a calendar date (Howard Hinnant's civil_from_days).
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

fn clip(text: &str, limit: usize) -> String {
    let flat: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    if flat.chars().count() <= limit {
        flat
    } else {
        format!("{}…", flat.chars().take(limit).collect::<String>())
    }
}

fn line(src: &str, chat: &str, kind: &str, detail: &str) -> String {
    serde_json::json!({ "at": now(), "src": src, "chat": chat, "kind": kind, "detail": clip(detail, 400) }).to_string()
}

fn write_all(path: &PathBuf, lines: &[String]) {
    if std::fs::metadata(path).is_ok_and(|meta| meta.len() > ROTATE_BYTES) {
        let _ = std::fs::rename(path, path.with_file_name("debug.1.log"));
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else { return };
    for line in lines {
        let _ = writeln!(file, "{}", line.replace(['\r', '\n'], " "));
    }
}

/// Starts the writer, the heartbeat and the panic hook, and registers the writer as the sink of the adapters' trace.
/// Called once at startup.
pub fn install(app: &AppHandle) {
    let Ok(path) = log_path(app) else { return };
    let (sender, receiver) = sync_channel::<String>(QUEUE);
    if WRITER.set(sender).is_err() || PATH.set(path.clone()).is_err() {
        return;
    }
    prune_old(&path);
    std::thread::spawn(move || {
        // Whatever is waiting goes out in one write.
        while let Ok(first) = receiver.recv() {
            let mut batch = vec![first];
            while let Ok(next) = receiver.try_recv() {
                batch.push(next);
                if batch.len() >= 512 {
                    break;
                }
            }
            write_all(&path, &batch);
        }
    });
    ag_adapters::agent::trace::set_sink(Arc::new(|chat, kind, detail| note("rust", chat, kind, detail)));
    // A panic is written at once (the writer thread may not get to it before the process goes), then handed on.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let entry = format!(
            "{}",
            serde_json::json!({ "at": now(), "src": "rust", "chat": "-", "kind": "PANIC", "detail": format!("{info} | thread {:?}", std::thread::current().name()), "backtrace": backtrace.to_string().chars().take(6000).collect::<String>() })
        );
        if let Some(path) = PATH.get() {
            write_all(path, &[entry]);
        }
        previous(info);
    }));
    std::thread::spawn(|| loop {
        std::thread::sleep(HEARTBEAT);
        heartbeat();
    });
    note(
        "rust",
        "-",
        "app started",
        &format!("v{} {} {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH),
    );
}

/// Notes one step. Never blocks: a full queue drops the note.
pub fn note(src: &str, chat: &str, kind: &str, detail: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    if let Some(writer) = WRITER.get() {
        let _ = writer.try_send(line(src, chat, kind, detail));
    }
}

/// A turn begins: from now on the heartbeat lists it.
pub fn turn_started(chat: &str, turn: &str, agent: &str, model: &str) {
    live().insert(
        (chat.to_owned(), turn.to_owned()),
        Live { agent: agent.to_owned(), started: Instant::now(), last_event: Instant::now(), last_kind: "start", events: 0, stream: None },
    );
    note("rust", chat, "turn start", &format!("turn={turn} agent={agent} model={model}"));
}

/// A turn's task ended (it finished, was stopped, or was aborted): `why` says which.
pub fn turn_ended(chat: &str, turn: &str, why: &str) {
    let mut turns = live();
    if let Some(entry) = turns.remove(&(chat.to_owned(), turn.to_owned())) {
        let pending = entry.stream;
        drop(turns);
        if let Some((kind, count, chars, _)) = pending {
            note("rust", chat, "stream", &format!("turn={turn} {kind} x{count} chars={chars}"));
        }
        note(
            "rust",
            chat,
            "turn end",
            &format!("turn={turn} why={why} after={:.1}s events={} last={}", entry.started.elapsed().as_secs_f32(), entry.events, entry.last_kind),
        );
    }
}

/// Lives as long as a turn's task: noted as started when it is made, and as ended when it is dropped, with `finish`'s reason
/// or, when the task was aborted before it got there, "aborted".
pub struct TurnGuard {
    chat: String,
    turn: String,
    why: &'static str,
}

impl TurnGuard {
    pub fn new(chat: &str, turn: &str, agent: &str, model: &str) -> TurnGuard {
        turn_started(chat, turn, agent, model);
        TurnGuard { chat: chat.to_owned(), turn: turn.to_owned(), why: "aborted" }
    }

    pub fn finish(&mut self, why: &'static str) {
        self.why = why;
    }
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        turn_ended(&self.chat, &self.turn, self.why);
    }
}

/// A short account of an event: its kind and what identifies it (never what the user wrote or the agent answered).
fn describe(event: &AgentEvent) -> (&'static str, String) {
    match event {
        AgentEvent::Delta { .. } => ("delta", String::new()),
        AgentEvent::Thinking { .. } => ("thinking", String::new()),
        AgentEvent::ToolCall { id, name, input, background, .. } => ("tool_call", format!("id={id} {name} bg={background} {}", clip(input, 120))),
        AgentEvent::ToolResult { id, output, is_error, .. } => ("tool_result", format!("id={id} error={is_error} bytes={}", output.len())),
        AgentEvent::ToolOutputDelta { .. } => ("tool_output", String::new()),
        AgentEvent::PermissionDenied { requests } => ("permission_denied", format!("{} request(s)", requests.len())),
        AgentEvent::Session { session_id } => ("session", format!("id={session_id}")),
        AgentEvent::Done { session_id, cost_usd, .. } => ("done", format!("session={} cost={cost_usd:?}", session_id.as_deref().unwrap_or("-"))),
        AgentEvent::Usage { .. } => ("usage", String::new()),
        AgentEvent::Error { message } => ("error", clip(message, 300)),
        AgentEvent::Raw { source, label, .. } => ("raw", format!("{source}/{label}")),
        AgentEvent::Scoped { parent, event } => {
            let (kind, detail) = describe(event);
            ("scoped", format!("parent={parent} {kind} {detail}"))
        }
        AgentEvent::Task { id, tool_use_id, status, label, tools, .. } => (
            "task",
            format!(
                "id={id} call={} {} {} tools={tools:?}",
                tool_use_id.as_deref().unwrap_or("-"),
                match status {
                    TaskStatus::Started => "started",
                    TaskStatus::Progress => "progress",
                    TaskStatus::Done => "done",
                    TaskStatus::Failed => "failed",
                },
                clip(label.as_deref().unwrap_or(""), 80)
            ),
        ),
        AgentEvent::AutoTurn { reason } => ("auto_turn", clip(reason, 120)),
    }
}

/// Every event sent to the chat goes through here. Streamed text (and usage, and tool output) is counted and written as
/// one line every `COALESCE`; everything else is a line of its own, after whatever was counted before it.
pub fn tap(chat: &str, turn: &str, event: &AgentEvent) {
    if WRITER.get().is_none() {
        return;
    }
    let (kind, detail) = describe(event);
    let size = match event {
        AgentEvent::Delta { text } | AgentEvent::Thinking { text } => text.chars().count(),
        AgentEvent::ToolOutputDelta { delta, .. } => delta.len(),
        _ => 0,
    };
    let streamed = matches!(kind, "delta" | "thinking" | "tool_output" | "usage");
    let mut out: Vec<String> = Vec::new();
    {
        let mut turns = live();
        let entry = turns.entry((chat.to_owned(), turn.to_owned())).or_insert_with(|| Live {
            agent: "?".to_owned(),
            started: Instant::now(),
            last_event: Instant::now(),
            last_kind: "start",
            events: 0,
            stream: None,
        });
        entry.last_event = Instant::now();
        entry.last_kind = kind;
        entry.events += 1;
        if streamed {
            let (_, count, chars, since) = entry.stream.get_or_insert((kind, 0, 0, Instant::now()));
            *count += 1;
            *chars += size;
            if since.elapsed() >= COALESCE {
                if let Some((stream_kind, count, chars, _)) = entry.stream.take() {
                    out.push(line("rust", chat, "stream", &format!("turn={turn} {stream_kind} x{count} chars={chars}")));
                }
            } else if let Some(open) = entry.stream.as_mut() {
                // A different kind of stream starts: the first one is written now.
                if open.0 != kind {
                    let (stream_kind, count, chars, _) = (open.0, open.1, open.2, open.3);
                    entry.stream = Some((kind, 1, size, Instant::now()));
                    out.push(line("rust", chat, "stream", &format!("turn={turn} {stream_kind} x{count} chars={chars}")));
                }
            }
        } else if let Some((stream_kind, count, chars, _)) = entry.stream.take() {
            out.push(line("rust", chat, "stream", &format!("turn={turn} {stream_kind} x{count} chars={chars}")));
        }
    }
    if !streamed {
        out.push(line("rust", chat, "event", &format!("turn={turn} {kind} {detail}")));
    }
    if let Some(writer) = WRITER.get() {
        for entry in out {
            let _ = writer.try_send(entry);
        }
    }
}

/// The turns that are running and how long each has been silent: a gap between two heartbeats means the whole process
/// stopped, a turn whose silence keeps growing is the one that hung.
fn heartbeat() {
    let report = {
        let turns = live();
        let parts: Vec<String> = turns
            .iter()
            .map(|((chat, turn), entry)| {
                format!(
                    "{chat}/{turn} {} age={}s silent={}s events={} last={}",
                    entry.agent,
                    entry.started.elapsed().as_secs(),
                    entry.last_event.elapsed().as_secs(),
                    entry.events,
                    entry.last_kind
                )
            })
            .collect();
        format!("running={} [{}] {}", turns.len(), parts.join("; "), ag_adapters::agent::trace::stats())
    };
    let page = PAGE_LAST.lock().ok().and_then(|seen| seen.as_ref().map(|(at, text)| (at.elapsed(), text.clone())));
    match page {
        Some((silent, last)) if silent >= PAGE_SILENT => {
            note("rust", "-", "page silent", &format!("no page line for {}s; page last wrote: {last}", silent.as_secs()));
            note("rust", "-", "heartbeat", &format!("{report} page_silent={}s", silent.as_secs()));
        }
        Some((silent, _)) => note("rust", "-", "heartbeat", &format!("{report} page_silent={}s", silent.as_secs())),
        None => note("rust", "-", "heartbeat", &format!("{report} page_silent=no page line yet")),
    }
}

/// Lines from the page (already JSON, one per entry).
#[tauri::command]
pub fn flow_log_append(lines: Vec<String>) {
    if let Some(last) = lines.last() {
        // The page writes at least every 20 s (5 s with the hang trace on): a long silence is a page that stopped.
        let text: String = last.chars().take(160).collect();
        if let Ok(mut seen) = PAGE_LAST.lock() {
            *seen = Some((Instant::now(), text));
        }
    }
    if let Some(writer) = WRITER.get() {
        if !ENABLED.load(Ordering::Relaxed) {
            return;
        }
        for entry in lines {
            let _ = writer.try_send(entry);
        }
    }
}

/// Turns the debug log on or off (Settings > Debug). Off, nothing is written to `debug.log`; `errors.log` is not touched.
#[tauri::command]
pub fn debug_log_set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

fn read_tail(path: &PathBuf, keep: usize) -> String {
    let mut text = std::fs::read_to_string(path.with_file_name("debug.1.log")).unwrap_or_default();
    text.push_str(&std::fs::read_to_string(path).unwrap_or_default());
    if text.len() > keep {
        let mut cut = text.len() - keep;
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        let from = text[cut..].find('\n').map_or(cut, |at| cut + at + 1);
        text = text[from..].to_string();
    }
    text
}

/// The end of the log (the older file first when the current one is short), as text.
#[tauri::command]
pub async fn flow_log_read(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || Ok(read_tail(&log_path(&app)?, READ_BYTES as usize)))
        .await
        .map_err(|error| error.to_string())?
}

/// Saves one file for a bug report where the user picks: this version and machine, the error log, the debug log (both
/// files). `None` when the dialog was cancelled.
#[tauri::command]
pub async fn debug_bundle_save(app: AppHandle, name: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let path = log_path(&app)?;
        let errors = path.with_file_name("errors.log");
        let mut text = format!(
            "AgentDeck debug bundle\nversion {}\n{} {}\nsaved {}\n\n== errors.log ==\n",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            now()
        );
        text.push_str(&std::fs::read_to_string(errors.with_file_name("errors.1.log")).unwrap_or_default());
        text.push_str(&std::fs::read_to_string(&errors).unwrap_or_default());
        text.push_str("\n== debug.log ==\n");
        text.push_str(&read_tail(&path, 64 * 1024 * 1024));
        let Some(destination) = app
            .dialog()
            .file()
            .set_title("Save the AgentDeck debug log")
            .set_file_name(name)
            .add_filter("Log", &["log", "txt"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let destination = destination.into_path().map_err(|error| error.to_string())?;
        std::fs::write(&destination, text).map_err(|error| error.to_string())?;
        Ok(Some(destination.display().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Where the debug log is kept, for "show in folder".
#[tauri::command]
pub fn debug_log_path(app: AppHandle) -> Result<String, String> {
    Ok(log_path(&app)?.display().to_string())
}
