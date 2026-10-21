use std::path::{Path, PathBuf};

use ag_adapters::history::{self, HistoryMessage, HistorySession, Roots, Source};

fn roots() -> Result<Roots, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or("cannot find the home folder")?;
    Ok(Roots::from_home(&home))
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
        let root = PathBuf::from(&root).canonicalize().map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path).canonicalize().map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("file is outside the project folder".to_string());
        }
        let size = std::fs::metadata(&path).map_err(|error| error.to_string())?.len();
        if size > MAX_BYTES {
            return Err(format!("file is {size} bytes, the viewer stops at 2 MB"));
        }
        std::fs::read_to_string(&path).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Bytes of one file for previews (images, PDF, SVG), base64-encoded. Same rules as `file_read`:
/// read-only, inside `root`, and at most 20 MB.
#[tauri::command]
async fn file_read_bytes(root: String, path: String) -> Result<String, String> {
    use base64::Engine;
    const MAX_BYTES: u64 = 20 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root).canonicalize().map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path).canonicalize().map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("file is outside the project folder".to_string());
        }
        let size = std::fs::metadata(&path).map_err(|error| error.to_string())?.len();
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
        .invoke_handler(tauri::generate_handler![
            history_list,
            history_load,
            history_rename,
            history_delete,
            file_read,
            file_read_bytes
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AgentDeck desktop application");
}
