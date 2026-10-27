//! The error log: one JSON line per problem the app met while it worked (a turn that failed, a step or a team member
//! that stopped, a hand-back that never ran, an agent quiet for too long, an uncaught error in the page). Kept on disk,
//! so a hang or two agents that could not reach each other can be traced after the fact. Part of the Audit log: the
//! Tasks view shows it in a drawer and saves it where the user picks.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// Past this size the log is moved to `errors.1.log` (one older file kept) and a new one starts.
const ROTATE_BYTES: u64 = 2 * 1024 * 1024;
/// What the drawer reads: the end of the log.
const READ_BYTES: u64 = 512 * 1024;

fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|error| error.to_string())?.join("logs");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

fn current(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(log_dir(app)?.join("errors.log"))
}

/// Appends lines (JSON objects, one per entry) to the log.
#[tauri::command]
pub async fn error_log_append(app: AppHandle, lines: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = current(&app)?;
        if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > ROTATE_BYTES) {
            let _ = std::fs::rename(&path, path.with_file_name("errors.1.log"));
        }
        let mut file = OpenOptions::new().create(true).append(true).open(&path).map_err(|error| error.to_string())?;
        for line in lines {
            // One entry per line, whatever the text holds.
            let line = line.replace(['\r', '\n'], " ");
            writeln!(file, "{line}").map_err(|error| error.to_string())?;
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// The end of the log (the older file first when the current one is short), as text.
#[tauri::command]
pub async fn error_log_read(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = current(&app)?;
        let mut text = std::fs::read_to_string(path.with_file_name("errors.1.log")).unwrap_or_default();
        text.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
        let keep = READ_BYTES as usize;
        if text.len() > keep {
            let mut cut = text.len() - keep;
            while !text.is_char_boundary(cut) {
                cut += 1;
            }
            // Start at a whole line.
            let from = text[cut..].find('\n').map_or(cut, |at| cut + at + 1);
            text = text[from..].to_string();
        }
        Ok(text)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Saves the whole log (older file included) where the user picks. `None` when the dialog was cancelled.
#[tauri::command]
pub async fn error_log_save(app: AppHandle, name: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let path = current(&app)?;
        let mut text = std::fs::read_to_string(path.with_file_name("errors.1.log")).unwrap_or_default();
        text.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
        let Some(destination) = app
            .dialog()
            .file()
            .set_title("Save the AgentDeck error log")
            .set_file_name(name)
            .add_filter("Log", &["log", "txt", "jsonl"])
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

/// Empties the log (both files).
#[tauri::command]
pub async fn error_log_clear(app: AppHandle) -> Result<(), String> {
    let path = current(&app)?;
    let _ = std::fs::remove_file(path.with_file_name("errors.1.log"));
    std::fs::write(&path, "").map_err(|error| error.to_string())
}
