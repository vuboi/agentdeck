use std::path::{Path, PathBuf};

mod bridge;
mod mcp;
mod index;
mod cdp;
mod browser;
mod servers;
mod detect;
mod connectors;
mod mcp_manage;
mod slash_commands;
mod pty;
mod changes;
mod custom_chat;
mod mcp_client;

use ag_adapters::agent::antigravity::Antigravity;
use ag_adapters::agent::claude::Claude;
use ag_adapters::agent::codex::Codex;
use ag_adapters::agent::runner::{drive_turn, resolve_binary};
use ag_adapters::agent::{AgentEvent, Provider, TurnRequest};
use ag_adapters::history::{self, HistoryMessage, HistorySession, Roots, Source};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

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
        let extension = match mime.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            other => return Err(format!("{other} is not a supported image type")),
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("the image is larger than 15 MB".to_string());
        }
        let folder = home_folder()?.join(".agentdeck").join("attachments");
        std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let path = folder.join(format!("image-{stamp:x}.{extension}"));
        std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The models an agent CLI offers, from its own cache on this machine.
#[tauri::command]
async fn models_list(agent: String) -> Vec<ag_adapters::models::ModelInfo> {
    // `agy models` asks the server: off the main thread.
    tauri::async_runtime::spawn_blocking(move || models_of(&agent)).await.unwrap_or_default()
}

fn models_of(agent: &str) -> Vec<ag_adapters::models::ModelInfo> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    match agent {
        "antigravity" => {
            let binary = resolve_binary(&Antigravity::default());
            ag_adapters::models::antigravity(&ag_adapters::agent::antigravity::models_listing(binary))
        }
        "claude" => ag_adapters::models::claude(&home),
        "codex" => {
            let codex_home = std::env::var_os("CODEX_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".codex"));
            ag_adapters::models::codex(&codex_home)
        }
        _ => Vec::new(),
    }
}

/// Sessions Claude Code and Codex already saved on this machine. Read-only.
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
    Ok(home_folder()?.join(".agentdeck").join("audit.jsonl"))
}

#[tauri::command]
fn audit_append(line: String) -> Result<(), String> {
    use std::io::Write;
    let path = audit_file()?;
    std::fs::create_dir_all(path.parent().ok_or("no folder for the audit log")?).map_err(|error| error.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    writeln!(file, "{line}").map_err(|error| error.to_string())
}

/// Everything in the Audit log; empty when nothing was recorded yet.
#[tauri::command]
fn audit_read() -> Result<String, String> {
    match std::fs::read_to_string(audit_file()?) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.to_string()),
    }
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
            let value: serde_json::Value = serde_json::from_str(line).unwrap_or(serde_json::Value::Null);
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
    match std::fs::remove_dir_all(audit_images()?) {
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
    Ok(home_folder()?.join(".agentdeck").join("audit-images"))
}

/// Saves a picture given as a data URL (`data:image/png;base64,…`). Returns the file's name.
#[tauri::command]
async fn audit_image_save(image: String) -> Result<String, String> {
    use base64::Engine;
    tauri::async_runtime::spawn_blocking(move || {
        let (head, data) = image.split_once(',').ok_or("not a data URL")?;
        let extension = match head {
            "data:image/png;base64" => "png",
            "data:image/jpeg;base64" => "jpg",
            "data:image/webp;base64" => "webp",
            "data:image/gif;base64" => "gif",
            other => return Err(format!("{other} is not a supported picture")),
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .map_err(|error| error.to_string())?;
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
        let bytes = std::fs::read(audit_images()?.join(&name)).map_err(|error| error.to_string())?;
        Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
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
    let safe = |part: &str| !part.is_empty() && part != "." && part != ".." && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !safe(run) || name.split('/').count() > 2 || !name.split('/').all(safe) {
        return Err("bad run file name".to_string());
    }
    let base = match root {
        Some(root) if !root.is_empty() => PathBuf::from(root),
        _ => home_folder()?,
    };
    Ok(base.join(".agentdeck").join("runs").join(run).join(name))
}

/// Writes one file of a run's notes and returns its full path. The runs folder ignores itself in git.
#[tauri::command]
async fn run_file_write(root: Option<String>, run: String, name: String, content: String) -> Result<String, String> {
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
}

/// The entries of one folder for the Explore panel: folders first, then files, by name. Same rule as `file_read`:
/// the folder must sit inside `root`. `.git` is left out.
#[tauri::command]
async fn folder_list(root: String, path: String) -> Result<Vec<FolderEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root).canonicalize().map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path).canonicalize().map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("folder is outside the project folder".to_string());
        }
        let mut entries: Vec<FolderEntry> = std::fs::read_dir(&path)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let dir = entry.file_type().ok()?.is_dir();
                (name != ".git").then_some(FolderEntry { name, dir })
            })
            .collect();
        entries.sort_by(|a, b| b.dir.cmp(&a.dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(entries)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// `path` inside `root` (not `root` itself), both made canonical.
fn inside_root(root: &str, path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(root).canonicalize().map_err(|error| error.to_string())?;
    let path = PathBuf::from(path).canonicalize().map_err(|error| error.to_string())?;
    if !path.starts_with(&root) || path == root {
        return Err("that is outside the project folder".to_string());
    }
    Ok(path)
}

/// Where deleted files go, so they can be put back (Ctrl+Z in Explore, or by hand).
fn file_trash() -> Result<PathBuf, String> {
    Ok(home_folder()?.join(".agentdeck").join("trash").join("files"))
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
        let name = from.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
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
        let trash = file_trash()?.canonicalize().map_err(|error| error.to_string())?;
        let trashed = PathBuf::from(&trashed).canonicalize().map_err(|error| error.to_string())?;
        if !trashed.starts_with(&trash) {
            return Err("that is not in AgentDeck's trash".to_string());
        }
        let target = PathBuf::from(&to);
        let parent = target.parent().ok_or("no folder to restore into")?;
        let root = PathBuf::from(&root).canonicalize().map_err(|error| error.to_string())?;
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
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_title("Add a project folder").blocking_pick_folder())
        .await
        .map_err(|error| error.to_string())?;
    match picked {
        Some(path) => path.into_path().map(|path| Some(path.display().to_string())).map_err(|error| error.to_string()),
        None => Ok(None),
    }
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
    std::process::Command::new(program).arg(&path).spawn().map(|_| ()).map_err(|error| error.to_string())
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

/// One event of one chat, sent to the UI on the `agent-event` channel.
#[derive(Clone, Serialize)]
struct AgentEnvelope {
    session: String,
    /// Which run of the chat the event belongs to, so a stopped run's last events are never mistaken for the next one's.
    turn: String,
    event: AgentEvent,
}

/// The runs in progress, per chat, so one can be stopped.
static RUNNING: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, tauri::async_runtime::JoinHandle<()>>>,
> = std::sync::LazyLock::new(Default::default);

fn provider_for(agent: &str) -> Result<Box<dyn Provider>, String> {
    match agent {
        "claude" => Ok(Box::new(Claude)),
        "codex" => Ok(Box::new(Codex::default())),
        "antigravity" => Ok(Box::new(Antigravity::default())),
        other => Err(format!("no agent provider for `{other}` yet")),
    }
}

/// Runs one turn for the app: resolves the binary, then forwards every event to the UI.
async fn run_turn(
    app: AppHandle,
    session: String,
    turn: String,
    provider: Box<dyn Provider>,
    req: TurnRequest,
) {
    if provider.id() == "antigravity" {
        mcp_manage::ensure_agy_link().await;
    }
    let binary = resolve_binary(provider.as_ref());
    drive_turn(binary, provider.as_ref(), &req, |event| {
        let _ = app.emit(
            "agent-event",
            AgentEnvelope {
                session: session.clone(),
                turn: turn.clone(),
                event,
            },
        );
    })
    .await;
}

/// Sends one user message to an agent CLI. Returns at once; the answer arrives as `agent-event`s.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // one parameter per field the frontend sends
async fn agent_send(
    app: AppHandle,
    session: String,
    turn: String,
    agent: String,
    prompt: String,
    agent_session: Option<String>,
    resume: bool,
    model: Option<String>,
    permission_mode: Option<String>,
    effort: Option<String>,
    allowed_tools: Vec<String>,
    cwd: Option<String>,
) -> Result<(), String> {
    let provider = provider_for(&agent)?;
    let req = TurnRequest {
        prompt,
        session_id: agent_session,
        resume,
        model,
        permission_mode,
        effort,
        allowed_tools,
        cwd: cwd.map(Into::into),
        owner: Some(session.clone()),
    };
    let handle = tauri::async_runtime::spawn(run_turn(app, session.clone(), turn, provider, req));
    if let Ok(mut running) = RUNNING.lock() {
        if let Some(previous) = running.insert(session, handle) {
            previous.abort(); // a chat runs one turn at a time
        }
    }
    Ok(())
}

/// Sends a message to a custom provider (an OpenAI-compatible server). Same contract as `agent_send`: returns
/// at once and the answer arrives as `agent-event`s. The whole conversation goes along: there is no CLI session.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // one parameter per field the frontend sends
async fn custom_send(
    app: AppHandle,
    session: String,
    turn: String,
    kind: String,
    base_url: String,
    api_key: Option<String>,
    model: String,
    messages: Vec<custom_chat::ChatMessage>,
    permission_mode: Option<String>,
    allowed_tools: Vec<String>,
    cwd: Option<String>,
    effort: Option<String>,
) -> Result<(), String> {
    let chat = session.clone();
    let config = custom_chat::Turn { kind, base_url, api_key, model, cwd: cwd.map(Into::into), permission_mode, allowed: allowed_tools, chat: session.clone(), effort };
    let handle = tauri::async_runtime::spawn(async move {
        custom_chat::run_turn(&app.clone(), config, messages, |event| {
            let _ = app.emit("agent-event", AgentEnvelope { session: chat.clone(), turn: turn.clone(), event });
        })
        .await;
    });
    if let Ok(mut running) = RUNNING.lock() {
        if let Some(previous) = running.insert(session, handle) {
            previous.abort();
        }
    }
    Ok(())
}

/// Stops the run of a chat: its CLI process is killed with the task.
#[tauri::command]
fn agent_stop(session: String) {
    if let Ok(mut running) = RUNNING.lock() {
        if let Some(handle) = running.remove(&session) {
            handle.abort();
        }
    }
}

/// Starts the AgentDeck desktop window.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .register_uri_scheme_protocol("agentdeck", |context, request| {
            // The Preview's own page for an address that did not answer.
            if request.uri().path() == "/error" {
                let text = request.uri().to_string();
                let target = tauri::Url::parse(&text)
                    .or_else(|_| tauri::Url::parse(&format!("http://localhost{text}")))
                    .ok()
                    .and_then(|url| url.query_pairs().find(|(key, _)| key == "url").map(|(_, value)| value.into_owned()))
                    .unwrap_or_default();
                return tauri::http::Response::builder()
                    .status(200)
                    .header("Content-Type", "text/html; charset=utf-8")
                    .body(browser::error_html(&target).into_bytes())
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
            bridge::start(app.handle().clone());
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
            history_list,
            changes::changes_list,
            changes::change_diff,
            changes::change_revert,
            changes::change_revert_all,
            changes::change_stage,
            connectors::connectors_detect,
            connectors::connector_install,
            connectors::connector_login,
            connectors::accounts_list,
            connectors::account_switch,
            connectors::account_add,
            connectors::agy_modes,
            mcp_manage::mcp_list,
            slash_commands::commands_list,
            slash_commands::command_read,
            mcp_manage::mcp_test,
            mcp_manage::mcp_add,
            mcp_manage::mcp_remove,
            mcp_manage::mcp_agy_link,
            connectors::antigravity_quota,
            connectors::account_adopt,
            connectors::account_remove,
            connectors::codex_auto_switch,
            connectors::provider_probe,
            connectors::local_scan,
            audit_append,
            audit_read,
            audit_clear,
            audit_image_save,
            audit_image_read,
            folder_list,
            reveal_path,
            audit_prune,
            pick_folder,
            path_rename,
            path_trash,
            path_restore,
            models_list,
            history_load,
            history_rename,
            history_delete,
            file_read,
            run_file_write,
            run_file_read,
            file_read_bytes,
            agent_send,
            custom_send,
            agent_stop,
            attachment_save,
            detect::detect_run_targets,
            servers::servers_list,
            servers::server_stop,
            detect::run_target,
            browser::browser_show,
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
            browser::browser_press,
            browser::open_external,
            pty::pty_open,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_close,
            pty::pty_default_folder
        ])
        .build(tauri::generate_context!())
        .expect("failed to build AgentDeck desktop application")
        .run(|_app, event| {
            // The dev servers the agents asked for end with the app.
            if let tauri::RunEvent::Exit = event {
                servers::kill_all();
                pty::kill_all();
            }
        });
}
