use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

mod agent_runs;
mod awake;
mod answers;
mod bridge;
mod channel;
mod browser;
mod browser_extensions;
mod cdp;
mod changes;
mod computer;
mod connectors;
mod copilot;
mod custom_chat;
mod delegate;
mod detect;
mod devtools;
mod endpoints;
mod error_log;
mod flow_log;
mod fs_ops;
mod image_backend;
mod image_gen;
mod image_history;
mod index;
mod mcp;
mod opencode_serve;
mod mcp_client;
mod mcp_manage;
mod mcp_providers;
mod notify;
mod page_memory;
mod picture_store;
mod path_links;
mod officecli;
mod process_guard;
mod process_registry;
mod project_memory;
mod tts;
mod privacy;
mod pty;
mod servers;
mod slash_commands;
mod store;

use ag_adapters::agent::antigravity::Antigravity;
use ag_adapters::agent::opencode::OpenCode;
use ag_adapters::agent::runner::resolve_binary;
use ag_adapters::history::{self, HistoryMessage, HistoryPage, HistorySession, Roots, Source};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
struct TrayState {
    icon: std::sync::Mutex<Option<tauri::tray::TrayIcon<tauri::Wry>>>,
    recent: std::sync::Mutex<Vec<TrayRecentSession>>,
}

#[derive(Clone, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrayRecentSession {
    id: String,
    title: String,
    agent: String,
}

fn tray_menu(
    app: &AppHandle,
    recent: &[TrayRecentSession],
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};

    let new_chat = MenuItem::with_id(app, "new-chat", "New chat", true, None::<&str>)?;
    let recent_heading =
        MenuItem::with_id(app, "recent-heading", "Recent chats", false, None::<&str>)?;
    let before_recent = PredefinedMenuItem::separator(app)?;
    let recent_items: Vec<_> = recent
        .iter()
        .take(6)
        .map(|session| {
            let mut title: String = session.title.chars().take(30).collect();
            if session.title.chars().count() > 30 {
                title.push('…');
            }
            MenuItem::with_id(
                app,
                format!("recent:{}", session.id),
                format!("{title} · {}", session.agent),
                true,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;
    let before_exit = PredefinedMenuItem::separator(app)?;
    let exit = MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;

    let mut items: Vec<&dyn IsMenuItem<tauri::Wry>> =
        vec![&new_chat, &before_recent, &recent_heading];
    items.extend(
        recent_items
            .iter()
            .map(|item| item as &dyn IsMenuItem<tauri::Wry>),
    );
    items.extend([&before_exit as &dyn IsMenuItem<tauri::Wry>, &exit]);
    Menu::with_items(app, &items)
}

#[tauri::command]
fn tray_update_recent(app: AppHandle, sessions: Vec<TrayRecentSession>) -> Result<(), String> {
    let state = app.state::<TrayState>();
    *state.recent.lock().map_err(|error| error.to_string())? = sessions.clone();
    app.emit("tray-recent-updated", &sessions)
        .map_err(|error| error.to_string())?;
    {
        let menu = tray_menu(&app, &sessions).map_err(|error| error.to_string())?;
        if let Some(tray) = state
            .icon
            .lock()
            .map_err(|error| error.to_string())?
            .as_ref()
        {
            tray.set_menu(Some(menu))
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn roots() -> Result<Roots, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or("cannot find the home folder")?;
    Ok(Roots::from_home(&home))
}

/// Saves a pasted or dropped image under `~/.agentdeck/attachments` so the agent CLI can open it by path.
/// `data` is the base64 of the file. Returns the path.
#[tauri::command]
async fn attachment_save(mime: String, data: String) -> Result<String, String> {
    use base64::Engine;
    const MAX_BYTES: usize = 15 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        if !matches!(mime.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/webp") {
            return Err(format!("{mime} is not a supported image type"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("the image is larger than 15 MB".to_string());
        }
        write_attachment(&mime, bytes)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// A picture already in memory (the browser's capture), stored like an attachment: WebP when it can be, the path back.
pub(crate) async fn save_attachment_bytes(mime: &str, bytes: Vec<u8>) -> Result<String, String> {
    let mime = mime.to_string();
    tauri::async_runtime::spawn_blocking(move || write_attachment(&mime, bytes))
        .await
        .map_err(|error| error.to_string())?
}

fn write_attachment(mime: &str, bytes: Vec<u8>) -> Result<String, String> {
    let (extension, bytes) = picture_store::stored_picture(mime, bytes);
    let folder = home_folder()?.join(crate::channel::home_folder_name()).join("attachments");
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let path = folder.join(format!("image-{stamp:x}.{extension}"));
    std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Saves a picked file (any type: document, spreadsheet, code) under `~/.agentdeck/attachments` so the agent CLI can
/// open it by path. `data` is the base64 of the file. Keeps the file's own name (made safe) and returns the path.
#[tauri::command]
async fn attachment_save_file(name: String, data: String) -> Result<String, String> {
    use base64::Engine;
    const MAX_BYTES: usize = 25 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("the file is larger than 25 MB".to_string());
        }
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let safe = safe.trim_matches(|c: char| c == '.' || c == ' ');
        let safe = if safe.is_empty() { "file" } else { safe };
        let folder = home_folder()?.join(crate::channel::home_folder_name()).join("attachments");
        std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let path = folder.join(format!("{stamp:x}-{safe}"));
        std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Saves a self-contained visual guide through the native Save As dialog.
#[tauri::command]
async fn browser_save_guide(app: AppHandle, title: String, html: String) -> Result<Option<String>, String> {
    const MAX_BYTES: usize = 120 * 1024 * 1024;
    if html.len() > MAX_BYTES {
        return Err("The guide is larger than 120 MB. Remove some video or screenshots and try again.".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;

        let mut name: String = title
            .chars()
            .map(|character| if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_') { character } else { '_' })
            .collect();
        name = name.trim().trim_matches('.').to_owned();
        if name.is_empty() { name = "user-guide".into(); }
        name.push_str(".html");
        let destination = app.dialog().file().set_title("Save visual guide").set_file_name(name).blocking_save_file();
        let Some(destination) = destination else { return Ok(None); };
        let destination = destination.into_path().map_err(|error| error.to_string())?;
        std::fs::write(&destination, html).map_err(|error| error.to_string())?;
        Ok(Some(destination.display().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Stores an agent-recorded WebM walkthrough in the project's guide folder without opening a dialog.
#[tauri::command]
async fn browser_save_recording(
    root: String,
    relative_path: String,
    data_url: String,
) -> Result<String, String> {
    const MAX_BYTES: usize = 500 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        use base64::Engine;

        let root = PathBuf::from(root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let relative = Path::new(&relative_path);
        if relative.is_absolute()
            || relative.components().any(|component| !matches!(component, std::path::Component::Normal(_)))
            || relative.extension() != Some(std::ffi::OsStr::new("webm"))
        {
            return Err("recording path must be a relative .webm file inside the project".into());
        }
        let (header, encoded) = data_url
            .split_once(',')
            .ok_or("recording data is malformed")?;
        if !header.starts_with("data:video/webm") || !header.ends_with(";base64") {
            return Err("recording must be a base64-encoded WebM video".into());
        }
        if encoded.len() > MAX_BYTES * 4 / 3 + 8 {
            return Err("recording exceeds the 500 MB size limit".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("recording exceeds the 500 MB size limit".into());
        }

        let file_name = relative.file_name().ok_or("recording filename is missing")?;
        if relative != Path::new("docs").join("user-guides").join(file_name) {
            return Err("recordings must be saved under docs/user-guides".into());
        }
        let docs = root.join("docs");
        let docs = match std::fs::symlink_metadata(&docs) {
            Ok(_) => docs.canonicalize().map_err(|error| error.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&docs).map_err(|error| error.to_string())?;
                docs.canonicalize().map_err(|error| error.to_string())?
            }
            Err(error) => return Err(error.to_string()),
        };
        if !docs.starts_with(&root) {
            return Err("recording folder is outside the project".into());
        }
        let guides = docs.join("user-guides");
        let parent = match std::fs::symlink_metadata(&guides) {
            Ok(_) => guides.canonicalize().map_err(|error| error.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&guides).map_err(|error| error.to_string())?;
                guides.canonicalize().map_err(|error| error.to_string())?
            }
            Err(error) => return Err(error.to_string()),
        };
        if !parent.starts_with(&root) {
            return Err("recording folder is outside the project".into());
        }
        let mut output = parent.join(file_name);
        if output.exists() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_millis());
            let stem = output.file_stem().unwrap_or_default().to_string_lossy();
            output.set_file_name(format!("{stem}-{stamp}.webm"));
        }
        let temp = output.with_extension("webm.agentdeck-tmp");
        if let Err(error) = std::fs::write(&temp, bytes).and_then(|()| std::fs::rename(&temp, &output)) {
            let _ = std::fs::remove_file(&temp);
            return Err(error.to_string());
        }
        Ok(output.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Resolves an agent's pending recording control request after the UI recorder handles it.
#[tauri::command]
fn browser_guide_record_reply(request_id: String, ok: bool, message: String) -> Result<(), String> {
    mcp::guide_recording_reply(request_id, ok, message)
}

/// Size of a file dropped onto the window (its path comes from the system's drop event), `None` for a folder.
#[tauri::command]
fn attachment_stat(path: String) -> Result<Option<u64>, String> {
    let meta = std::fs::metadata(&path).map_err(|error| error.to_string())?;
    Ok(if meta.is_file() {
        Some(meta.len())
    } else {
        None
    })
}

/// A picture a tool saved on disk (a screenshot), as a data URL for the chat to show. Image files only, at most 15 MB.
#[tauri::command]
async fn read_image_data_url(path: String) -> Result<String, String> {
    use base64::Engine;
    const MAX_BYTES: u64 = 15 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let mime = match Path::new(&path)
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase())
            .as_deref()
        {
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            _ => return Err("not an image file".to_string()),
        };
        let meta = std::fs::metadata(&path).map_err(|error| error.to_string())?;
        if !meta.is_file() || meta.len() > MAX_BYTES {
            return Err("not a file, or larger than 8 MB".to_string());
        }
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        Ok(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The raw bytes of an attached file, for the document preview (sent as binary, not JSON). At most 50 MB.
#[tauri::command]
async fn read_file_bytes(path: String) -> Result<tauri::ipc::Response, String> {
    const MAX_BYTES: u64 = 50 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let meta = std::fs::metadata(&path).map_err(|error| error.to_string())?;
        if !meta.is_file() || meta.len() > MAX_BYTES {
            return Err("not a file, or larger than 50 MB".to_string());
        }
        std::fs::read(&path)
            .map(tauri::ipc::Response::new)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Shows an attached file in the system file manager, selected.
#[tauri::command]
fn attachment_reveal(path: String) -> Result<(), String> {
    if !Path::new(&path).is_file() {
        return Err(format!("{path} is not a file"));
    }
    let mut command = if cfg!(windows) {
        let mut command = std::process::Command::new("explorer");
        command.arg(format!("/select,{path}"));
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg("-R").arg(&path);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(Path::new(&path).parent().unwrap_or_else(|| Path::new("/")));
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// The models an agent CLI offers, from its own cache on this machine.
#[tauri::command]
async fn models_list(agent: String) -> Vec<ag_adapters::models::ModelInfo> {
    if agent == "copilot" {
        return copilot::models_list().await.unwrap_or_default();
    }
    // `agy models` asks the server: off the main thread.
    tauri::async_runtime::spawn_blocking(move || models_of(&agent))
        .await
        .unwrap_or_default()
}

fn models_of(agent: &str) -> Vec<ag_adapters::models::ModelInfo> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    match agent {
        "antigravity" => {
            let binary = resolve_binary(&Antigravity::default());
            ag_adapters::models::antigravity(&ag_adapters::agent::antigravity::models_listing(
                binary,
            ))
        }
        "claude" => ag_adapters::models::claude(&home),
        "opencode" => {
            let binary = resolve_binary(&OpenCode::default());
            let cache = std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".cache"))
                .join("opencode")
                .join("models.json");
            ag_adapters::models::opencode(
                &ag_adapters::agent::opencode::models_listing(binary),
                std::fs::read_to_string(cache).ok().as_deref(),
            )
        }
        "codex" => {
            let codex_home = std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".codex"));
            ag_adapters::models::codex(&codex_home)
        }
        _ => Vec::new(),
    }
}

/// The last `lines` lines of the log of the server on `port` (colour codes kept), for the Tasks panel.
#[tauri::command]
fn server_log(port: u16, lines: usize) -> String {
    let log = servers::read_log(port);
    let all: Vec<&str> = log.lines().collect();
    let skip = all.len().saturating_sub(lines.max(1));
    let tail = all[skip..].join("\n");
    let start = tail.len().saturating_sub(20 * 1024);
    let start = (start..=tail.len())
        .find(|index| tail.is_char_boundary(*index))
        .unwrap_or(tail.len());
    let tail = &tail[start..];
    if skip > 0 || start > 0 {
        format!("(Earlier output not shown)\n{tail}")
    } else {
        tail.to_string()
    }
}

/// Indexes the conversations saved by Claude Code and Codex. `is_project` is decided by the indexer (one check per folder).
#[tauri::command]
async fn history_list() -> Result<Vec<HistorySession>, String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || history::list_all(&roots))
        .await
        .map_err(|error| error.to_string())
}

/// The transcript of one saved session. The path must be inside the history folders.
#[tauri::command]
async fn history_load(source: Source, path: String) -> Result<Vec<HistoryMessage>, String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || history::load(&roots, source, Path::new(&path)))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

/// A window of the transcript of one saved session: the newest `limit` messages, or the ones before `before`.
#[tauri::command]
async fn history_load_page(
    source: Source,
    path: String,
    before: Option<usize>,
    limit: usize,
) -> Result<HistoryPage, String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || {
        history::load_page(&roots, source, Path::new(&path), before, limit)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

/// Renames a saved session inside Claude's or Codex's own storage.
#[tauri::command]
async fn history_rename(source: Source, path: String, title: String) -> Result<(), String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || {
        history::rename(&roots, source, Path::new(&path), &title)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

pub(crate) fn home_folder() -> Result<PathBuf, String> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or_else(|| "cannot find the home folder".to_string())
}

/// The Audit log: `~/.agentdeck/audit.jsonl`, one JSON line per finished tool call or chat error.
fn audit_file() -> Result<PathBuf, String> {
    Ok(home_folder()?.join(crate::channel::home_folder_name()).join("audit.jsonl"))
}

/// Per-day line counts of the Audit log, kept beside it so a count does not read the whole file.
/// `length` is the log's size it was made for and `offset_min` the time zone (JS `getTimezoneOffset`) the days are cut by:
/// when either no longer matches (a crash between write and index, an edit by hand, a clock change) the index is rebuilt.
#[derive(serde::Serialize, serde::Deserialize, Default, Clone, Copy)]
struct AuditDay {
    total: usize,
    failed: usize,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct AuditIndex {
    length: u64,
    offset_min: i64,
    days: std::collections::BTreeMap<i64, AuditDay>,
}

/// Counts one entry (its JSON line) into its day: every call, and the failed ones apart.
fn count_audit_line(days: &mut std::collections::BTreeMap<i64, AuditDay>, line: &str, offset_min: i64) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else { return };
    let Some(started) = value.get("startedAt").and_then(serde_json::Value::as_f64) else { return };
    let day = days.entry(audit_day(started, offset_min as f64)).or_default();
    day.total += 1;
    if value.get("status").and_then(serde_json::Value::as_str) == Some("fail") {
        day.failed += 1;
    }
}

const DAY_MS: f64 = 86_400_000.0;

/// The local day number of a time (ms since the epoch) for a time zone offset given in JS minutes (positive west of UTC).
fn audit_day(started: f64, offset_min: f64) -> i64 {
    ((started - offset_min * 60_000.0) / DAY_MS).floor() as i64
}

fn audit_index_file() -> Result<PathBuf, String> {
    Ok(home_folder()?.join(crate::channel::home_folder_name()).join("audit.index.json"))
}

/// Serialises every change to the index: appends, counts and rebuilds all read and write it.
static AUDIT_INDEX_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn audit_index_lock() -> std::sync::MutexGuard<'static, ()> {
    AUDIT_INDEX_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn read_audit_index() -> Option<AuditIndex> {
    let text = std::fs::read_to_string(audit_index_file().ok()?).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_audit_index(index: &AuditIndex) -> Result<(), String> {
    let path = audit_index_file()?;
    let temp = path.with_extension("json.tmp");
    let text = serde_json::to_string(index).map_err(|error| error.to_string())?;
    std::fs::write(&temp, text).map_err(|error| error.to_string())?;
    std::fs::rename(&temp, &path).map_err(|error| error.to_string())
}

/// The index for the log as it is now: the saved one when it still matches the file, else rebuilt from the file.
fn audit_index_now(offset_min: i64) -> Result<AuditIndex, String> {
    let path = audit_file()?;
    let length = match std::fs::metadata(&path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(AuditIndex { offset_min, ..Default::default() }),
        Err(error) => return Err(error.to_string()),
    };
    if let Some(index) = read_audit_index().filter(|index| index.length == length && index.offset_min == offset_min) {
        return Ok(index);
    }
    let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut index = AuditIndex { length: text.len() as u64, offset_min, days: Default::default() };
    for line in text.lines() {
        count_audit_line(&mut index.days, line, offset_min);
    }
    save_audit_index(&index)?;
    Ok(index)
}

#[tauri::command]
fn audit_append(line: String, offset_min: i64) -> Result<(), String> {
    use std::io::Write;
    let _guard = audit_index_lock();
    let path = audit_file()?;
    std::fs::create_dir_all(path.parent().ok_or("no folder for the audit log")?)
        .map_err(|error| error.to_string())?;
    let before = std::fs::metadata(&path).map(|metadata| metadata.len()).unwrap_or(0);
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    writeln!(file, "{line}").map_err(|error| error.to_string())?;
    // The index follows the append only while it was up to date before it; otherwise the next count rebuilds it.
    if let Some(mut index) = read_audit_index().filter(|index| index.length == before && index.offset_min == offset_min) {
        count_audit_line(&mut index.days, &line, offset_min);
        index.length = std::fs::metadata(&path).map(|metadata| metadata.len()).unwrap_or(0);
        save_audit_index(&index)?;
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditPageEntry {
    offset: u64,
    line: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditPage {
    entries: Vec<AuditPageEntry>,
    cursor: Option<u64>,
    has_more: bool,
}

fn audit_previous_newline(file: &mut std::fs::File, before: u64) -> std::io::Result<Option<u64>> {
    use std::io::{Read, Seek, SeekFrom};

    const CHUNK: u64 = 8192;
    let mut end = before;
    while end > 0 {
        let start = end.saturating_sub(CHUNK);
        let mut bytes = vec![0; (end - start) as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut bytes)?;
        if let Some(index) = bytes.iter().rposition(|byte| *byte == b'\n') {
            return Ok(Some(start + index as u64));
        }
        end = start;
    }
    Ok(None)
}

/// Reads a bounded page of the Audit log, newest first on disk and chronological within each returned page.
/// `cursor` is the byte offset of the oldest line in the previous page; appends do not invalidate it.
#[tauri::command]
async fn audit_read(cursor: Option<u64>, limit: usize) -> Result<AuditPage, String> {
    use std::io::{Read, Seek, SeekFrom};

    tauri::async_runtime::spawn_blocking(move || {
        let path = audit_file()?;
        let mut file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AuditPage { entries: Vec::new(), cursor: None, has_more: false });
            }
            Err(error) => return Err(error.to_string()),
        };
        let length = file.metadata().map_err(|error| error.to_string())?.len();
        let mut end = cursor.unwrap_or(length).min(length);
        if end > 0 {
            file.seek(SeekFrom::Start(end - 1)).map_err(|error| error.to_string())?;
            let mut last = [0];
            file.read_exact(&mut last).map_err(|error| error.to_string())?;
            if last[0] == b'\n' {
                end -= 1;
            }
        }

        let page_size = limit.clamp(1, 100);
        let mut entries = Vec::with_capacity(page_size + 1);
        while end > 0 && entries.len() <= page_size {
            let separator = audit_previous_newline(&mut file, end).map_err(|error| error.to_string())?;
            let start = separator.map_or(0, |offset| offset + 1);
            if start < end {
                let mut bytes = vec![0; (end - start) as usize];
                file.seek(SeekFrom::Start(start)).map_err(|error| error.to_string())?;
                file.read_exact(&mut bytes).map_err(|error| error.to_string())?;
                if !bytes.iter().all(u8::is_ascii_whitespace)
                    && serde_json::from_slice::<serde_json::Value>(&bytes).is_ok()
                {
                    entries.push(AuditPageEntry {
                        offset: start,
                        line: String::from_utf8_lossy(&bytes).into_owned(),
                    });
                }
            }
            let Some(separator) = separator else { break };
            end = separator;
        }

        let has_more = entries.len() > page_size;
        entries.truncate(page_size);
        entries.reverse();
        let cursor = has_more.then(|| entries.first().map(|entry| entry.offset)).flatten();
        Ok(AuditPage { entries, cursor, has_more })
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditDayCount {
    day: i64,
    total: usize,
    failed: usize,
}

/// The full count of every day the Audit log holds (its local day number, all calls, failed calls), so the headers show
/// the whole day from the start and not only the pages read so far.
#[tauri::command]
async fn audit_days(offset_min: i64) -> Result<Vec<AuditDayCount>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = audit_index_lock();
        let index = audit_index_now(offset_min)?;
        Ok(index
            .days
            .into_iter()
            .map(|(day, count)| AuditDayCount { day, total: count.total, failed: count.failed })
            .collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Keeps only the Audit log lines that started at or after `cutoff` (ms since the epoch; the app passes the start of
/// yesterday, so two days are kept), and deletes the pictures no kept line points to. Returns how many lines went.
#[tauri::command]
async fn audit_prune(cutoff: f64) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = audit_file()?;
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error.to_string()),
        };
        let mut kept = Vec::new();
        let mut pictures = std::collections::HashSet::new();
        let mut dropped = 0;
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            let value: serde_json::Value =
                serde_json::from_str(line).unwrap_or(serde_json::Value::Null);
            // A line it cannot read, or without a time, is kept: only what is known to be old goes.
            let started = value.get("startedAt").and_then(serde_json::Value::as_f64);
            if started.is_some_and(|started| started < cutoff) {
                dropped += 1;
                continue;
            }
            if let Some(image) = value.get("image").and_then(serde_json::Value::as_str) {
                pictures.insert(image.to_string());
            }
            kept.push(line);
        }
        if dropped > 0 {
            let mut body = kept.join("\n");
            if !body.is_empty() {
                body.push('\n');
            }
            std::fs::write(&path, body).map_err(|error| error.to_string())?;
        }
        if let Ok(entries) = std::fs::read_dir(audit_images()?) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !pictures.contains(&name) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        Ok(dropped)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
fn audit_clear() -> Result<(), String> {
    let _guard = audit_index_lock();
    match std::fs::remove_dir_all(audit_images()?) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    match std::fs::remove_file(audit_index_file()?) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    match std::fs::remove_file(audit_file()?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// Pictures a tool call returned (a screenshot of the page): `~/.agentdeck/audit-images`, one file each. The log
/// line keeps the file's name, not the picture, so the log stays small.
fn audit_images() -> Result<PathBuf, String> {
    Ok(home_folder()?.join(crate::channel::home_folder_name()).join("audit-images"))
}

/// Saves a picture given as a data URL (`data:image/png;base64,…`). Returns the file's name.
#[tauri::command]
async fn audit_image_save(image: String) -> Result<String, String> {
    use base64::Engine;
    tauri::async_runtime::spawn_blocking(move || {
        let (head, data) = image.split_once(',').ok_or("not a data URL")?;
        let mime = head
            .strip_prefix("data:")
            .and_then(|rest| rest.strip_suffix(";base64"))
            .ok_or_else(|| format!("{head} is not a supported picture"))?;
        if !matches!(mime, "image/png" | "image/jpeg" | "image/gif" | "image/webp") {
            return Err(format!("{mime} is not a supported picture"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .map_err(|error| error.to_string())?;
        let (extension, bytes) = picture_store::stored_picture(mime, bytes);
        let folder = audit_images()?;
        std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let name = format!("shot-{stamp:x}.{extension}");
        std::fs::write(folder.join(&name), bytes).map_err(|error| error.to_string())?;
        Ok(name)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The full path of a saved picture, for the asset protocol (no bytes cross IPC). Only a bare file name of that folder.
#[tauri::command]
fn audit_image_path(name: String) -> Result<String, String> {
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return Err("not a picture of the audit log".to_string());
    }
    let path = audit_images()?.join(&name);
    if !path.is_file() {
        return Err("not a picture of the audit log".to_string());
    }
    Ok(path.to_string_lossy().into_owned())
}

/// A saved picture as a data URL. Only a bare file name of that folder is read.
#[tauri::command]
async fn audit_image_read(name: String) -> Result<String, String> {
    use base64::Engine;
    tauri::async_runtime::spawn_blocking(move || {
        if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
            return Err("not a picture of the audit log".to_string());
        }
        let mime = match name.rsplit('.').next() {
            Some("png") => "image/png",
            Some("jpg") => "image/jpeg",
            Some("webp") => "image/webp",
            Some("gif") => "image/gif",
            _ => return Err("not a picture of the audit log".to_string()),
        };
        let bytes =
            std::fs::read(audit_images()?.join(&name)).map_err(|error| error.to_string())?;
        Ok(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Moves a saved session to `~/.agentdeck/trash` (not destroyed), so it leaves Claude and Codex too.
#[tauri::command]
async fn history_delete(source: Source, path: String) -> Result<(), String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || {
        history::delete(&roots, source, Path::new(&path)).map(|_| ())
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

/// Text of one file for the file viewer. Read-only, text only, at most 2 MB, and the file must sit
/// inside `root` (the project folder), so a path from the UI cannot reach outside it.
#[tauri::command]
async fn file_read(root: String, path: String) -> Result<String, String> {
    const MAX_BYTES: u64 = 2 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("file is outside the project folder".to_string());
        }
        let size = std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len();
        if size > MAX_BYTES {
            return Err(format!("file is {size} bytes, the viewer stops at 2 MB"));
        }
        std::fs::read_to_string(&path).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The folder of one workflow run: `<root>/.agentdeck/runs/<run>`, or under the home folder when the chat has no
/// project. It holds the run's shared notes: `BOARD.md` (written by the app), `agents/<name>.md` (each agent its own)
/// and `handoff/<n>-<name>.md` (what a step hands to the next).
fn run_file(root: Option<String>, run: &str, name: &str) -> Result<PathBuf, String> {
    let safe = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if !safe(run) || name.split('/').count() > 2 || !name.split('/').all(safe) {
        return Err("bad run file name".to_string());
    }
    let base = match root {
        Some(root) if !root.is_empty() => PathBuf::from(root),
        _ => home_folder()?,
    };
    Ok(base.join(crate::channel::home_folder_name()).join("runs").join(run).join(name))
}

/// Writes one file of a run's notes and returns its full path. The runs folder ignores itself in git.
#[tauri::command]
async fn run_file_write(
    root: Option<String>,
    run: String,
    name: String,
    content: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let file = run_file(root, &run, &name)?;
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        }
        let runs = file
            .ancestors()
            .find(|path| path.file_name().is_some_and(|name| name == "runs"))
            .map(|path| path.join(".gitignore"));
        if let Some(runs) = runs {
            if !runs.exists() {
                std::fs::write(&runs, "*\n").map_err(|error| error.to_string())?;
            }
        }
        std::fs::write(&file, content).map_err(|error| error.to_string())?;
        Ok(file.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// One file of a run's notes as it is now (the last 64 KB when longer); empty when the agent has not written it.
#[tauri::command]
async fn run_file_read(root: Option<String>, run: String, name: String) -> Result<String, String> {
    const MAX_BYTES: usize = 64 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let file = run_file(root, &run, &name)?;
        let Ok(text) = std::fs::read_to_string(file) else {
            return Ok(String::new());
        };
        if text.len() <= MAX_BYTES {
            return Ok(text);
        }
        let mut start = text.len() - MAX_BYTES;
        while !text.is_char_boundary(start) {
            start += 1;
        }
        Ok(text[start..].to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[derive(serde::Serialize)]
struct FolderEntry {
    name: String,
    dir: bool,
    has_children: bool,
}

/// The entries of one folder for the Explore panel: folders first, then files, by name. Same rule as `file_read`:
/// the folder must sit inside `root`. `.git` is left out.
#[tauri::command]
async fn folder_list(root: String, path: String) -> Result<Vec<FolderEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("folder is outside the project folder".to_string());
        }
        let mut entries: Vec<FolderEntry> = std::fs::read_dir(&path)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let dir = entry.file_type().ok()?.is_dir();
                let has_children = if dir {
                    std::fs::read_dir(entry.path())
                        .map(|children| {
                            children.filter_map(Result::ok).any(|child| {
                                child.file_name().to_string_lossy() != ".git"
                            })
                        })
                        .unwrap_or(true)
                } else {
                    false
                };
                (name != ".git").then_some(FolderEntry {
                    name,
                    dir,
                    has_children,
                })
            })
            .collect();
        entries.sort_by(|a, b| {
            b.dir
                .cmp(&a.dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// `path` inside `root` (not `root` itself), both made canonical.
fn inside_root(root: &str, path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let path = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !path.starts_with(&root) || path == root {
        return Err("that is outside the project folder".to_string());
    }
    Ok(path)
}

/// Where deleted files go, so they can be put back (Ctrl+Z in Explore, or by hand).
fn file_trash() -> Result<PathBuf, String> {
    Ok(home_folder()?
        .join(crate::channel::home_folder_name())
        .join("trash")
        .join("files"))
}

fn copy_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_all(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

/// Moves a file or folder; across drives (the trash is on the home drive) it copies, then removes the original.
fn move_path(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_all(from, to).map_err(|error| error.to_string())?;
    if from.is_dir() {
        std::fs::remove_dir_all(from)
    } else {
        std::fs::remove_file(from)
    }
    .map_err(|error| error.to_string())
}

/// Renames a file or folder of the project in place.
#[tauri::command]
async fn path_rename(root: String, path: String, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let name = name.trim().to_string();
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err(format!("“{name}” is not a valid name"));
        }
        let from = inside_root(&root, &path)?;
        let to = from.with_file_name(&name);
        if to.exists() {
            return Err(format!("{name} already exists here"));
        }
        std::fs::rename(&from, &to).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Moves a file or folder of the project to AgentDeck's trash; answers where it went, to put it back.
#[tauri::command]
async fn path_trash(root: String, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let from = inside_root(&root, &path)?;
        let folder = file_trash()?;
        std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let name = from
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let to = folder.join(format!("{stamp:x}-{name}"));
        move_path(&from, &to)?;
        Ok(to.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Puts a trashed file or folder back at `to`, inside the project, if nothing has taken its place.
#[tauri::command]
async fn path_restore(root: String, trashed: String, to: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let trash = file_trash()?
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let trashed = PathBuf::from(&trashed)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !trashed.starts_with(&trash) {
            return Err("that is not in AgentDeck's trash".to_string());
        }
        let target = PathBuf::from(&to);
        let parent = target.parent().ok_or("no folder to restore into")?;
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let parent = parent.canonicalize().map_err(|error| error.to_string())?;
        if !parent.starts_with(&root) {
            return Err("that is outside the project folder".to_string());
        }
        let target = parent.join(target.file_name().ok_or("no name to restore")?);
        if target.exists() {
            return Err(format!("{} already exists", target.display()));
        }
        move_path(&trashed, &target)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Asks for a folder with the system's own picker, for a new project. `None` when the picker is cancelled.
#[tauri::command]
async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Add a project folder")
            .blocking_pick_folder()
    })
    .await
    .map_err(|error| error.to_string())?;
    match picked {
        Some(path) => path
            .into_path()
            .map(|path| Some(path.display().to_string()))
            .map_err(|error| error.to_string()),
        None => Ok(None),
    }
}

/// Asks for an application executable with the system picker. `None` when the picker is cancelled.
#[tauri::command]
async fn file_pick_app(app: AppHandle, path: String) -> Result<Option<String>, String> {
    if !Path::new(&path).is_file() {
        return Err("file does not exist".to_string());
    }
    use tauri_plugin_dialog::DialogExt;
    let dialog_app = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let dialog = dialog_app
            .dialog()
            .file()
            .set_title("Choose an application");
        #[cfg(windows)]
        let dialog = dialog.add_filter("Applications", &["exe"]);
        dialog.blocking_pick_file()
    })
    .await
    .map_err(|error| error.to_string())?;
    match picked {
        Some(picked) => {
            let app_path = picked
                .into_path()
                .map_err(|error| error.to_string())?;
            if !app_path.is_file() {
                return Err("application path is not a file".to_string());
            }
            persist_file_open_app(&app, &path, &app_path)?;
            Ok(Some(app_path.display().to_string()))
        }
        None => Ok(None),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileOpenOption {
    id: String,
    label: String,
    icon: Option<String>,
}

static FILE_APP_ICONS: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

fn app_icon_cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    FILE_APP_ICONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn image_data_url(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let mime = match extension.as_str() {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        _ => return None,
    };
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    use base64::Engine;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(windows)]
fn extract_windows_app_icon(path: &Path) -> Option<String> {
    use base64::Engine;
    let encoded_path = base64::engine::general_purpose::STANDARD
        .encode(path.as_os_str().to_string_lossy().as_bytes());
    let script = format!(
        "$p=[Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{encoded_path}')); Add-Type -AssemblyName System.Drawing; try {{ $i=[System.Drawing.Icon]::ExtractAssociatedIcon($p); if ($null -eq $i) {{ exit 1 }}; $b=$i.ToBitmap(); $s=New-Object System.IO.MemoryStream; $b.Save($s,[System.Drawing.Imaging.ImageFormat]::Png); [Console]::Out.Write([Convert]::ToBase64String($s.ToArray())); $s.Dispose(); $b.Dispose(); $i.Dispose() }} catch {{ exit 1 }}"
    );
    let output = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let encoded = String::from_utf8(output.stdout).ok()?;
    let encoded = encoded.trim();
    if encoded.is_empty() {
        return None;
    }
    Some(format!("data:image/png;base64,{encoded}"))
}

#[cfg(target_os = "macos")]
fn macos_app_icon(label: &str) -> Option<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let roots = [
        Some(PathBuf::from("/Applications")),
        Some(PathBuf::from("/System/Applications")),
        Some(PathBuf::from("/Applications/Utilities")),
        home.map(|path| path.join("Applications")),
    ];
    let app = roots
        .into_iter()
        .flatten()
        .map(|root| root.join(format!("{label}.app")))
        .find(|path| path.is_dir())?;
    let resources = app.join("Contents/Resources");
    let mut candidates = std::fs::read_dir(&resources)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "icns" | "png"))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|path| {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
        (!name.contains("appicon"), !name.contains("icon"), name)
    });
    let source = candidates.into_iter().next()?;
    if source.extension().and_then(|value| value.to_str()) == Some("png") {
        return image_data_url(&source);
    }
    let output = std::env::temp_dir().join(format!("agentdeck-app-icon-{}.png", uuid::Uuid::new_v4()));
    let result = std::process::Command::new("sips")
        .args(["-s", "format", "png"])
        .arg(&source)
        .arg("--out")
        .arg(&output)
        .output()
        .ok()?;
    if !result.status.success() {
        let _ = std::fs::remove_file(output);
        return None;
    }
    let data = image_data_url(&output);
    let _ = std::fs::remove_file(output);
    data
}

#[cfg(target_os = "linux")]
fn linux_app_icon(app_id: &str) -> Option<String> {
    let desktop_ids: &[&str] = match app_id {
        "gedit" => &["org.gnome.gedit", "gedit"],
        "evince" => &["org.gnome.Evince", "evince"],
        "eog" => &["org.gnome.eog", "eog"],
        _ => &[app_id],
    };
    let mut data_dirs = Vec::new();
    if let Some(home) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        data_dirs.push(home);
    } else if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        data_dirs.push(home.join(".local/share"));
    }
    data_dirs.extend(
        std::env::var_os("XDG_DATA_DIRS")
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into())
            .to_string_lossy()
            .split(':')
            .filter(|path| !path.is_empty())
            .map(PathBuf::from),
    );
    let desktop = data_dirs
        .iter()
        .flat_map(|root| desktop_ids.iter().map(move |id| root.join(format!("applications/{id}.desktop"))))
        .find(|path| path.is_file())?;
    let contents = std::fs::read_to_string(desktop).ok()?;
    let icon = contents
        .lines()
        .find_map(|line| line.strip_prefix("Icon=").map(str::trim))?;
    let icon_path = Path::new(icon);
    if icon_path.is_absolute() {
        return image_data_url(icon_path);
    }
    let names = [format!("{icon}.png"), format!("{icon}.svg"), format!("{icon}.jpg")];
    for root in &data_dirs {
        for name in &names {
            for size in ["scalable/apps", "256x256/apps", "128x128/apps", "96x96/apps", "64x64/apps", "48x48/apps", "32x32/apps", "24x24/apps", "16x16/apps"] {
                let candidate = root.join("icons/hicolor").join(size).join(name);
                if candidate.is_file() {
                    return image_data_url(&candidate);
                }
            }
            let pixmap = root.join("pixmaps").join(name);
            if pixmap.is_file() {
                return image_data_url(&pixmap);
            }
        }
    }
    None
}

fn icon_for_app(path: &Path, app_id: &str, _label: &str) -> Option<String> {
    let key = format!("{}|{app_id}", path.display());
    if let Ok(cache) = app_icon_cache().lock() {
        if let Some(icon) = cache.get(&key) {
            return icon.clone();
        }
    }
    #[cfg(windows)]
    let icon = extract_windows_app_icon(path);
    #[cfg(target_os = "macos")]
    let icon = macos_app_icon(_label);
    #[cfg(target_os = "linux")]
    let icon = linux_app_icon(app_id);
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    let icon: Option<String> = None;
    if let Ok(mut cache) = app_icon_cache().lock() {
        cache.insert(key, icon.clone());
    }
    icon
}

#[derive(serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedFileOpenApp {
    path: String,
    label: String,
}

fn file_open_apps_config(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("open-with.json"))
}

fn saved_file_open_apps(app: &AppHandle) -> std::collections::HashMap<String, SavedFileOpenApp> {
    let Some(config) = file_open_apps_config(app) else {
        return std::collections::HashMap::new();
    };
    std::fs::read_to_string(config)
        .ok()
        .and_then(|contents| serde_json::from_str(&contents).ok())
        .unwrap_or_default()
}

fn persist_file_open_app(app: &AppHandle, file: &str, program: &Path) -> Result<(), String> {
    let extension = Path::new(file)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension.is_empty() {
        return Ok(());
    }
    let config = file_open_apps_config(app).ok_or("could not locate AgentDeck config folder")?;
    let parent = config.parent().ok_or("invalid AgentDeck config folder")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut saved = saved_file_open_apps(app);
    saved.insert(
        extension,
        SavedFileOpenApp {
            path: program.display().to_string(),
            label: program
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        },
    );
    let contents = serde_json::to_vec_pretty(&saved).map_err(|error| error.to_string())?;
    std::fs::write(config, contents).map_err(|error| error.to_string())
}

/// Returns installed applications that make sense for this file type, plus the OS app chooser.
#[tauri::command]
async fn file_open_options(app: AppHandle, path: String) -> Result<Vec<FileOpenOption>, String> {
    // Windows icon extraction starts PowerShell for each detected editor. Keep filesystem probing and
    // icon extraction off the webview/main thread so merely opening a tab context menu stays responsive.
    tauri::async_runtime::spawn_blocking(move || file_open_options_sync(app, path))
        .await
        .map_err(|error| error.to_string())?
}

fn file_open_options_sync(app: AppHandle, path: String) -> Result<Vec<FileOpenOption>, String> {
    if !Path::new(&path).is_file() {
        return Err("file does not exist".to_string());
    }
    let extension = Path::new(&path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let text = matches!(
        extension.as_str(),
        "txt"
            | "md"
            | "rs"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "svelte"
            | "json"
            | "toml"
            | "yaml"
            | "yml"
            | "html"
            | "css"
            | "scss"
            | "py"
            | "go"
            | "java"
            | "cs"
            | "sql"
            | "sh"
            | "ps1"
            | "log"
            | "xml"
            | "csv"
    );
    let image = matches!(
        extension.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico"
    );
    let pdf = extension == "pdf";
    let word = matches!(extension.as_str(), "doc" | "docx" | "docm" | "dot" | "dotx" | "rtf");
    let excel = matches!(extension.as_str(), "xls" | "xlsx" | "xlsm" | "xlsb" | "xltx");
    let powerpoint = matches!(extension.as_str(), "ppt" | "pptx" | "pptm" | "ppsx" | "potx");
    let mut options = Vec::new();
    if let Some(saved) = saved_file_open_apps(&app).get(&extension) {
        if Path::new(&saved.path).is_file() {
            options.push(FileOpenOption {
                id: format!("custom:{}", saved.path),
                label: saved.label.clone(),
                icon: icon_for_app(Path::new(&saved.path), "custom", &saved.label),
            });
        }
    }
    #[cfg(windows)]
    {
        let mut candidates: Vec<(&str, &str, Vec<PathBuf>)> = vec![];
        let local = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_default();
        let programs = std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .unwrap_or_default();
        let programs_x86 = std::env::var_os("ProgramFiles(x86)")
            .map(PathBuf::from)
            .unwrap_or_default();
        if word || excel || powerpoint {
            let (app, label, exe) = if word {
                ("word", "Microsoft Word", "WINWORD.EXE")
            } else if excel {
                ("excel", "Microsoft Excel", "EXCEL.EXE")
            } else {
                ("powerpoint", "Microsoft PowerPoint", "POWERPNT.EXE")
            };
            let paths = [
                programs.join(format!("Microsoft Office/root/Office16/{exe}")),
                programs_x86.join(format!("Microsoft Office/root/Office16/{exe}")),
                programs.join(format!("Microsoft Office/Office16/{exe}")),
                programs_x86.join(format!("Microsoft Office/Office16/{exe}")),
            ];
            if paths.iter().any(|candidate| candidate.is_file()) || which_app(exe).is_some() {
                candidates.push((app, label, paths.to_vec()));
            }
        }
        if text {
            candidates.extend([
                (
                    "code",
                    "Visual Studio Code",
                    vec![local.join("Programs/Microsoft VS Code/Code.exe")],
                ),
                ("zed", "Zed", vec![local.join("Programs/Zed/Zed.exe")]),
                (
                    "cursor",
                    "Cursor",
                    vec![local.join("Programs/Cursor/Cursor.exe")],
                ),
                (
                    "notepadpp",
                    "Notepad++",
                    vec![programs.join("Notepad++/notepad++.exe")],
                ),
                ("notepad", "Notepad", vec![PathBuf::from("notepad.exe")]),
            ]);
        }
        if image {
            candidates.extend([
                ("paint", "Paint", vec![PathBuf::from("mspaint.exe")]),
                (
                    "photos",
                    "Photos",
                    vec![local.join("Microsoft/WindowsApps/Photos.exe")],
                ),
            ]);
        }
        if pdf {
            candidates.extend([
                (
                    "edge",
                    "Microsoft Edge",
                    vec![
                        programs.join("Microsoft/Edge/Application/msedge.exe"),
                        PathBuf::from("msedge.exe"),
                    ],
                ),
                (
                    "acrobat",
                    "Adobe Acrobat",
                    vec![programs.join("Adobe/Acrobat DC/Acrobat/Acrobat.exe")],
                ),
            ]);
        }
        for (id, label, paths) in candidates {
            let executable = paths
                .iter()
                .find(|candidate| candidate.is_file())
                .cloned()
                .or_else(|| which_app(id));
            if let Some(executable) = executable {
                options.push(FileOpenOption {
                    id: id.to_string(),
                    label: label.to_string(),
                    icon: icon_for_app(&executable, id, label),
                });
            }
        }
        options.push(FileOpenOption {
            id: "system".to_string(),
            label: "Open with Windows…".to_string(),
            icon: None,
        });
    }
    #[cfg(target_os = "macos")]
    {
        let candidates: &[(&str, &str, &str)] = if text {
            &[
                ("code", "Visual Studio Code", "Visual Studio Code"),
                ("zed", "Zed", "Zed"),
                ("cursor", "Cursor", "Cursor"),
                ("textedit", "TextEdit", "TextEdit"),
            ]
        } else if image {
            &[
                ("preview", "Preview", "Preview"),
                ("photos", "Photos", "Photos"),
            ]
        } else if pdf {
            &[
                ("preview", "Preview", "Preview"),
                ("chrome", "Google Chrome", "Google Chrome"),
            ]
        } else {
            &[]
        };
        for (id, label, app_name) in candidates {
            if std::process::Command::new("open")
                .args(["-Ra", app_name])
                .status()
                .is_ok_and(|status| status.success())
            {
                options.push(FileOpenOption {
                    id: (*id).to_string(),
                    label: (*label).to_string(),
                    icon: icon_for_app(Path::new(app_name), id, label),
                });
            }
        }
        options.push(FileOpenOption {
            id: "system".to_string(),
            label: "Open with system app".to_string(),
            icon: None,
        });
    }
    #[cfg(target_os = "linux")]
    {
        let candidates: &[(&str, &str)] = if text {
            &[
                ("code", "Visual Studio Code"),
                ("zed", "Zed"),
                ("cursor", "Cursor"),
                ("gedit", "Text Editor"),
            ]
        } else if image {
            &[("gimp", "GIMP"), ("eog", "Image Viewer")]
        } else if pdf {
            &[
                ("evince", "Document Viewer"),
                ("okular", "Okular"),
                ("firefox", "Firefox"),
            ]
        } else {
            &[]
        };
        for (id, label) in candidates {
            if which_app(id).is_some() {
                options.push(FileOpenOption {
                    id: (*id).to_string(),
                    label: (*label).to_string(),
                    icon: icon_for_app(Path::new(id), id, label),
                });
            }
        }
        options.push(FileOpenOption {
            id: "system".to_string(),
            label: "Open with system app".to_string(),
            icon: None,
        });
    }
    Ok(options)
}

/// Finds an application binary on PATH (including common Windows executable suffixes).
fn which_app(name: &str) -> Option<PathBuf> {
    let suffixes: &[&str] = if cfg!(windows) { &[".exe", ""] } else { &[""] };
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).find_map(|dir| {
        suffixes
            .iter()
            .map(|suffix| dir.join(format!("{name}{suffix}")))
            .find(|candidate| candidate.is_file())
    })
}

/// Opens a file in a detected application or the operating system's app chooser.
#[tauri::command]
async fn file_open_with(app: AppHandle, path: String, app_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || file_open_with_sync(app, path, app_id))
        .await
        .map_err(|error| error.to_string())?
}

fn file_open_with_sync(app: AppHandle, path: String, app_id: String) -> Result<(), String> {
    if !Path::new(&path).is_file() {
        return Err("file does not exist".to_string());
    }
    #[cfg(windows)]
    let result = {
        if let Some(program) = app_id.strip_prefix("custom:") {
            let program = PathBuf::from(program);
            if !program.is_file() {
                return Err("application is not available".to_string());
            }
            std::process::Command::new(program).arg(&path).spawn()
        } else if app_id == "system" {
            std::process::Command::new("rundll32.exe")
                .args(["shell32.dll,OpenAs_RunDLL", &path])
                .spawn()
        } else {
            let options = file_open_options_sync(app.clone(), path.clone())?;
            if !options
                .iter()
                .any(|option| option.id == app_id && app_id != "system")
            {
                return Err("application is not available for this file".to_string());
            }
            let local = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_default();
            let programs = std::env::var_os("ProgramFiles")
                .map(PathBuf::from)
                .unwrap_or_default();
            let programs_x86 = std::env::var_os("ProgramFiles(x86)")
                .map(PathBuf::from)
                .unwrap_or_default();
            let candidates = match app_id.as_str() {
                "code" => vec![
                    local.join("Programs/Microsoft VS Code/Code.exe"),
                    PathBuf::from("code.exe"),
                ],
                "zed" => vec![local.join("Programs/Zed/Zed.exe"), PathBuf::from("zed.exe")],
                "cursor" => vec![
                    local.join("Programs/Cursor/Cursor.exe"),
                    PathBuf::from("cursor.exe"),
                ],
                "notepadpp" => vec![
                    programs.join("Notepad++/notepad++.exe"),
                    PathBuf::from("notepad++.exe"),
                ],
                "notepad" => vec![PathBuf::from("notepad.exe")],
                "paint" => vec![PathBuf::from("mspaint.exe")],
                "photos" => vec![local.join("Microsoft/WindowsApps/Photos.exe")],
                "edge" => vec![
                    programs.join("Microsoft/Edge/Application/msedge.exe"),
                    PathBuf::from("msedge.exe"),
                ],
                "acrobat" => vec![programs.join("Adobe/Acrobat DC/Acrobat/Acrobat.exe")],
                "word" => vec![
                    programs.join("Microsoft Office/root/Office16/WINWORD.EXE"),
                    programs_x86.join("Microsoft Office/root/Office16/WINWORD.EXE"),
                    programs.join("Microsoft Office/Office16/WINWORD.EXE"),
                    programs_x86.join("Microsoft Office/Office16/WINWORD.EXE"),
                    PathBuf::from("WINWORD.EXE"),
                ],
                "excel" => vec![
                    programs.join("Microsoft Office/root/Office16/EXCEL.EXE"),
                    programs_x86.join("Microsoft Office/root/Office16/EXCEL.EXE"),
                    programs.join("Microsoft Office/Office16/EXCEL.EXE"),
                    programs_x86.join("Microsoft Office/Office16/EXCEL.EXE"),
                    PathBuf::from("EXCEL.EXE"),
                ],
                "powerpoint" => vec![
                    programs.join("Microsoft Office/root/Office16/POWERPNT.EXE"),
                    programs_x86.join("Microsoft Office/root/Office16/POWERPNT.EXE"),
                    programs.join("Microsoft Office/Office16/POWERPNT.EXE"),
                    programs_x86.join("Microsoft Office/Office16/POWERPNT.EXE"),
                    PathBuf::from("POWERPNT.EXE"),
                ],
                _ => return Err("unsupported application".to_string()),
            };
            let program = candidates
                .into_iter()
                .find(|candidate| candidate.is_file())
                .or_else(|| which_app(&app_id))
                .ok_or("application is not installed")?;
            std::process::Command::new(program).arg(&path).spawn()
        }
    };
    #[cfg(target_os = "macos")]
    let result = {
        let _ = app;
        let app = match app_id.as_str() {
            "system" => None,
            "code" => Some("Visual Studio Code"),
            "zed" => Some("Zed"),
            "cursor" => Some("Cursor"),
            "textedit" => Some("TextEdit"),
            "preview" => Some("Preview"),
            "photos" => Some("Photos"),
            "chrome" => Some("Google Chrome"),
            value if value.starts_with("custom:") => None,
            _ => return Err("unsupported application".to_string()),
        };
        let custom = app_id.strip_prefix("custom:");
        let mut command = std::process::Command::new("open");
        if let Some(program) = custom {
            command.args(["-a", program]);
        }
        if let Some(app) = app {
            command.args(["-a", app]);
        }
        command.arg(&path).spawn()
    };
    #[cfg(target_os = "linux")]
    let result = {
        let _ = app;
        if let Some(program) = app_id.strip_prefix("custom:") {
            let program = PathBuf::from(program);
            if !program.is_file() {
                return Err("application is not available".to_string());
            }
            std::process::Command::new(program).arg(&path).spawn()
        } else if app_id == "system" {
            std::process::Command::new("xdg-open").arg(&path).spawn()
        } else {
            let app = match app_id.as_str() {
                "code" => "code",
                "zed" => "zed",
                "cursor" => "cursor",
                "gedit" => "gedit",
                "gimp" => "gimp",
                "eog" => "eog",
                "evince" => "evince",
                "okular" => "okular",
                "firefox" => "firefox",
                _ => return Err("unsupported application".to_string()),
            };
            let program = which_app(app).ok_or("application is not installed")?;
            std::process::Command::new(program).arg(&path).spawn()
        }
    };
    result.map(|_| ()).map_err(|error| error.to_string())
}

/// Copies a project file to a destination chosen with the native Save As dialog.
#[tauri::command]
async fn file_save_as(
    app: AppHandle,
    root: String,
    path: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let source = PathBuf::from(path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !source.starts_with(&root) || !source.is_file() {
            return Err("file is outside the project folder".to_string());
        }
        let name = source
            .file_name()
            .ok_or("file has no name")?
            .to_string_lossy()
            .to_string();
        let destination = app
            .dialog()
            .file()
            .set_title("Save file as")
            .set_file_name(name)
            .blocking_save_file();
        let Some(destination) = destination else {
            return Ok(None);
        };
        let destination = destination.into_path().map_err(|error| error.to_string())?;
        std::fs::copy(&source, &destination).map_err(|error| error.to_string())?;
        Ok(Some(destination.display().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Saves an edited screenshot (a PNG, base64 without a prefix) where the user picks, with the native Save As dialog.
/// Answers the chosen path, or `None` when the dialog was cancelled.
#[tauri::command]
async fn image_save_as(app: AppHandle, name: String, data: String) -> Result<Option<String>, String> {
    use base64::Engine;
    use tauri_plugin_dialog::DialogExt;
    const MAX_BYTES: usize = 100 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("the image is larger than 100 MB".to_string());
        }
        let destination = app
            .dialog()
            .file()
            .set_title("Save screenshot")
            .set_file_name(name)
            .add_filter("PNG image", &["png"])
            .blocking_save_file();
        let Some(destination) = destination else {
            return Ok(None);
        };
        let destination = destination.into_path().map_err(|error| error.to_string())?;
        std::fs::write(&destination, bytes).map_err(|error| error.to_string())?;
        Ok(Some(destination.display().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Copies a file an agent made (a sound, a video, a document) to where the user picks, with the native Save As dialog.
/// Answers the chosen path, or `None` when the dialog was cancelled.
#[tauri::command]
async fn file_copy_as(app: AppHandle, path: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let source = PathBuf::from(&path);
        if !source.is_file() {
            return Err(format!("{path} is not a file"));
        }
        let name = source
            .file_name()
            .ok_or("file has no name")?
            .to_string_lossy()
            .to_string();
        let destination = app
            .dialog()
            .file()
            .set_title("Save file as")
            .set_file_name(name)
            .blocking_save_file();
        let Some(destination) = destination else {
            return Ok(None);
        };
        let destination = destination.into_path().map_err(|error| error.to_string())?;
        std::fs::copy(&source, &destination).map_err(|error| error.to_string())?;
        Ok(Some(destination.display().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Saves an export of AgentDeck's settings (JSON text) where the user picks, with the native Save As dialog.
/// Answers the chosen path, or `None` when the dialog was cancelled.
#[tauri::command]
async fn settings_export(app: AppHandle, name: String, text: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let destination = app
            .dialog()
            .file()
            .set_title("Export AgentDeck data")
            .set_file_name(name)
            .add_filter("AgentDeck export", &["json"])
            .blocking_save_file();
        let Some(destination) = destination else {
            return Ok(None);
        };
        let destination = destination.into_path().map_err(|error| error.to_string())?;
        std::fs::write(&destination, text).map_err(|error| error.to_string())?;
        Ok(Some(destination.display().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Writes a portable chat transcript under AgentDeck's app-data folder and returns its absolute path.
#[tauri::command]
async fn conversation_export(app: AppHandle, name: String, text: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let folder = app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?
            .join("conversations");
        std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
        let stem = name.strip_suffix(".md").unwrap_or(&name);
        let safe: String = stem
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        let filename = if safe.is_empty() {
            "conversation.md".to_string()
        } else {
            format!("{safe}.md")
        };
        let destination = folder.join(filename);
        std::fs::write(&destination, text).map_err(|error| error.to_string())?;
        Ok(destination.display().to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Reads an export the user picks with the native Open dialog. `None` when the dialog was cancelled.
#[tauri::command]
async fn settings_import(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    const MAX_BYTES: u64 = 50 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let source = app
            .dialog()
            .file()
            .set_title("Import AgentDeck data")
            .add_filter("AgentDeck export", &["json"])
            .blocking_pick_file();
        let Some(source) = source else {
            return Ok(None);
        };
        let source = source.into_path().map_err(|error| error.to_string())?;
        let size = std::fs::metadata(&source).map_err(|error| error.to_string())?.len();
        if size > MAX_BYTES {
            return Err("the file is larger than 50 MB: it is not an AgentDeck export".to_string());
        }
        std::fs::read_to_string(&source).map(Some).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Opens a folder in the system file manager (Explorer, Finder, or the desktop's own).
#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    if !Path::new(&path).is_dir() {
        return Err(format!("{path} is not a folder"));
    }
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program)
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Bytes of one file for previews (images, PDF, SVG), base64-encoded. Same rules as `file_read`:
/// read-only, inside `root`, and at most 20 MB.
#[tauri::command]
async fn file_read_bytes(root: String, path: String) -> Result<String, String> {
    use base64::Engine;
    const MAX_BYTES: u64 = 20 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("file is outside the project folder".to_string());
        }
        let size = std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len();
        if size > MAX_BYTES {
            return Err(format!("file is {size} bytes, the preview stops at 20 MB"));
        }
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Starts the AgentDeck desktop window.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .register_uri_scheme_protocol("agentdeck", |context, request| {
            // The Preview's own page for an address that did not answer.
            if request.uri().path() == "/error" {
                let text = request.uri().to_string();
                let query = tauri::Url::parse(&text)
                    .or_else(|_| tauri::Url::parse(&format!("http://localhost{text}")))
                    .ok();
                let param = |name: &str| {
                    query
                        .as_ref()
                        .and_then(|url| {
                            url.query_pairs()
                                .find(|(key, _)| key == name)
                                .map(|(_, value)| value.into_owned())
                        })
                        .unwrap_or_default()
                };
                return tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", "text/html; charset=utf-8")
                    .body(browser::error_html(&param("url"), &param("reason")).into_bytes())
                    .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()));
            }
            browser::handle_report(context.app_handle(), request.body());
            tauri::http::Response::builder()
                .status(200)
                .header("Access-Control-Allow-Origin", "*")
                .body(Vec::new())
                .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
        })
        .setup(|app| {
            // First: every folder and MCP name below depends on which build this is.
            channel::init(&app.config().identifier);
            flow_log::install(app.handle());
            // Create the main webview with the same WebView2 extension setting used by Preview child views.
            let main_window = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .ok_or_else(|| std::io::Error::other("the main window configuration is missing"))?;
            tauri::WebviewWindowBuilder::from_config(app.handle(), main_window)?
                .title(app.config().product_name.as_deref().unwrap_or("AgentDeck"))
                .browser_extensions_enabled(browser::EXTENSIONS_ENABLED)
                .build()?;

            app.manage(TrayState::default());
            let tray_menu = tray_menu(app.handle(), &[])?;
            let tray_icon = app
                .default_window_icon()
                .cloned()
                .ok_or_else(|| std::io::Error::other("the AgentDeck tray icon is missing"))?;
            let tray_builder = tauri::tray::TrayIconBuilder::new()
                .icon(tray_icon)
                .tooltip(app.config().product_name.as_deref().unwrap_or("AgentDeck"))
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    let id = event.id().as_ref();
                    if id == "exit" {
                        app.exit(0);
                    } else if id == "new-chat" {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                        let _ =
                            app.emit("tray-action", serde_json::json!({ "action": "new-chat" }));
                    } else if let Some(session_id) = id.strip_prefix("recent:") {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                        let _ = app.emit(
                        "tray-action",
                        serde_json::json!({ "action": "open-session", "sessionId": session_id }),
                    );
                    }
                });
            let tray = tray_builder
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button,
                        button_state,
                        ..
                    } = event
                    {
                        if button_state == tauri::tray::MouseButtonState::Up
                            && button == tauri::tray::MouseButton::Left
                        {
                            if let Some(window) = tray.app_handle().get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;
            if let Ok(mut slot) = app.state::<TrayState>().icon.lock() {
                *slot = Some(tray);
            }
            bridge::start(app.handle().clone());
            if let Ok(dir) = app.path().resource_dir() {
                image_gen::set_resource_dir(dir);
            }
            // Processes a previous run of the app left behind (it was killed or crashed) are ended before anything starts.
            if let Ok(data) = app.path().app_data_dir() {
                process_registry::init(data);
                process_registry::reap(process_guard::kill_tree_now);
            }
            let handle = app.handle().clone();
            servers::init(move |port| {
                let _ = handle.emit("server-log", serde_json::json!({ "port": port }));
            });
            servers::adopt_agents();
            if let Ok(url) = std::env::var("AGENTDECK_BROWSER_SELFTEST") {
                browser::selftest(app.handle().clone(), url);
            }
            // Developer hook: open the Preview tool on a page a few seconds after start-up.
            if let Ok(url) = std::env::var("AGENTDECK_OPEN_PREVIEW") {
                let handle = app.handle().clone();
                tauri::Listener::listen(app.handle(), "browser-report", |event| {
                    let shown: String = event.payload().chars().take(140).collect();
                    eprintln!("report: {shown}");
                });
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    let _ = tauri::Emitter::emit(&handle, "dev-preview", url);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tray_update_recent,
            history_list,
            changes::changes_list,
            changes::change_diff,
            changes::change_revert,
            changes::change_revert_all,
            changes::change_stage,
            changes::git_branches,
            changes::git_switch,
            connectors::connectors_detect,
            connectors::connector_install,
            connectors::connector_login,
            connectors::connector_login_cancel,
            connectors::accounts_list,
            connectors::codex_consume_reset_credit,
            connectors::account_switch,
            connectors::account_add,
            connectors::account_add_cancel,
            connectors::agy_modes,
            mcp_manage::mcp_list,
            slash_commands::commands_list,
            slash_commands::command_read,
            store::store_installed,
            store::store_scan_path,
            store::store_stage,
            store::store_review,
            store::store_install_staged,
            store::store_discard_staged,
            store::store_pick_folder,
            store::store_catalog_page,
            store::store_catalog_refresh,
            store::store_catalog_install,
            store::store_sources_list,
            store::store_source_add,
            store::store_source_remove,
            store::store_updates_check,
            store::maintenance::store_skill_source,
            store::maintenance::store_mcp_updates,
            store::maintenance::store_mcp_update,
            mcp_providers::mcp_source,
            store::store_update_skill,
            store::sync::store_sync,
            store::categories::store_categories,
            store::store_plugin_share,
            store::store_mcp_login,
            store::store_skill_import,
            store::store_skill_assign,
            store::store_skill_remove,
            store::store_plugin_set,
            store::store_plugin_uninstall,
            mcp_manage::mcp_test,
            mcp_manage::mcp_add,
            mcp_providers::mcp_provider_set,
            mcp_manage::mcp_remove,
            mcp_manage::mcp_agy_link,
            connectors::antigravity_quota,
            connectors::antigravity_quota_cached,
            connectors::antigravity_quotas,
            connectors::secret_save,
            connectors::secret_load,
            connectors::secret_delete,
            connectors::antigravity_accounts_list,
            connectors::antigravity_account_adopt,
            connectors::antigravity_account_switch,
            connectors::account_adopt,
            connectors::account_remove,
            connectors::codex_auto_switch,
            connectors::provider_probe,
            connectors::provider_quota,
            connectors::claude_console_credit,
            notify::system_notify,
            error_log::error_log_append,
            error_log::error_log_read,
            error_log::error_log_save,
            error_log::error_log_clear,
            flow_log::flow_log_append,
            flow_log::flow_log_read,
            flow_log::debug_bundle_save,
            flow_log::debug_log_path,
            connectors::local_scan,
            audit_append,
            audit_read,
            audit_days,
            audit_clear,
            audit_image_save,
            audit_image_read,
            audit_image_path,
            folder_list,
            reveal_path,
            path_links::path_resolve,
            path_links::path_open,
            path_links::path_reveal,
            path_links::image_fetch,
            officecli::officecli_status,
            officecli::officecli_install,
            officecli::officecli_remove,
            tts::tts_status,
            tts::tts_install,
            tts::tts_remove,
            tts::tts_speak,
            project_memory::project_memory_read,
            project_memory::project_memory_write,
            file_open_options,
            file_open_with,
            file_save_as,
            image_save_as,
            file_copy_as,
            settings_export,
            conversation_export,
            settings_import,
            audit_prune,
            pick_folder,
            file_pick_app,
            path_rename,
            path_trash,
            path_restore,
            fs_ops::path_create,
            fs_ops::project_files,
            fs_ops::file_write,
            models_list,
            history_load,
            history_load_page,
            history_rename,
            history_delete,
            file_read,
            run_file_write,
            run_file_read,
            file_read_bytes,
            agent_runs::agent_send,
            copilot::copilot_permission_answer,
            agent_runs::agent_warm_reset,
            agent_runs::agent_prewarm,
            awake::keep_awake_set,
            agent_runs::custom_send,
            agent_runs::prehandle_attachments,
            agent_runs::agent_stop,
            agent_runs::agent_stop_all,
            agent_runs::agent_inject,
            delegate::delegate_reply,
            delegate::delegate_ready,
            delegate::delegate_enter,
            delegate::delegate_release,
            page_memory::page_memory_reply,
            attachment_save,
            attachment_save_file,
            browser_save_guide,
            browser_save_recording,
            browser_guide_record_reply,
            attachment_stat,
            attachment_reveal,
            read_image_data_url,
            read_file_bytes,
            image_gen::image_generate,
            image_gen::image_prompt_suggest,
            image_gen::image_stop,
            image_history::image_history_list,
            detect::detect_run_targets,
            servers::servers_list,
            servers::server_stop,
            server_log,
            detect::run_target,
            browser::browser_show,
            browser::browser_pick_extension,
            browser::browser_add_extension,
            browser_extensions::browser_extensions,
            browser_extensions::browser_extension_enable,
            browser_extensions::browser_extension_reload,
            browser_extensions::browser_extension_remove,
            browser::browser_bounds,
            browser::browser_hide,
            browser::browser_navigate,
            browser::browser_reset,
            browser::browser_park,
            browser::browser_layer,
            browser::browser_eval,
            browser::browser_zoom,
            browser::devtools_cdp,
            browser::browser_close,
            browser::browser_capture,
            browser::browser_element_capture,
        browser::browser_capture_file,
        browser::browser_element_capture_file,
            browser::browser_press,
            browser::open_external,
            browser::browser_favicon,
            browser::browser_suggest,
            computer::computer_use_set_enabled,
            computer::computer_use_approval,
            pty::pty_open,
            pty::pty_shells,
            pty::pty_attach,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_close,
            pty::pty_default_folder
        ])
        .build(tauri::generate_context!())
        .expect("failed to build AgentDeck desktop application")
        .run(|app, event| match event {
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::CloseRequested { api, .. },
                ..
            } if label == "main" => {
                api.prevent_close();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            // The dev servers the agents asked for end only when the user chooses Exit from the tray.
            tauri::RunEvent::Exit => {
                // End idle agent CLIs as well as the child servers when the user exits.
                ag_adapters::agent::warm::reset(None);
                servers::kill_all();
                pty::kill_all();
                awake::release();
            }
            _ => {}
        });
}
