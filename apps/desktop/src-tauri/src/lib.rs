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

/// Starts the AgentDeck desktop window.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![history_list, history_load])
        .run(tauri::generate_context!())
        .expect("failed to run AgentDeck desktop application");
}
