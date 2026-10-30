// A small memory file per project (`<project>/.agentdeck/memory.md`) that every agent working there is told about: facts
// about the project that are worth keeping between sessions. The agents read it from the prompt and add to it with their
// own file tools; the Settings page reads and writes it here. Improvements an agent thinks are better are not written
// there directly: they go to `<project>/.agentdeck/proposals.md` as a bullet each, and the user keeps or drops them.
use std::path::{Path, PathBuf};

const FILE: &str = ".agentdeck/memory.md";
const PROPOSALS: &str = ".agentdeck/proposals.md";
/// Far above what is sent to an agent (project-memory.ts keeps the first few KB); the cap only stops a runaway file.
const MAX_BYTES: usize = 64 * 1024;

fn file_in(root: &Path, name: &str) -> Result<PathBuf, String> {
    if !root.is_dir() {
        return Err(format!("{} is not a folder", root.display()));
    }
    Ok(root.join(name))
}

fn read_file(root: &Path, name: &str) -> Result<String, String> {
    let file = file_in(root, name)?;
    match std::fs::read(&file) {
        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_BYTES)]).into_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.to_string()),
    }
}

/// Replaces the file `name` of a project. Empty text removes it.
fn write_file(root: &Path, name: &str, text: &str) -> Result<(), String> {
    let file = file_in(root, name)?;
    if text.len() > MAX_BYTES {
        return Err(format!("memory is larger than {} KB", MAX_BYTES / 1024));
    }
    if text.trim().is_empty() {
        return match std::fs::remove_file(&file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
            _ => Ok(()),
        };
    }
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    std::fs::write(&file, text).map_err(|error| error.to_string())
}

/// The memory of a project; empty when there is none yet.
pub fn read_in(root: &Path) -> Result<String, String> {
    read_file(root, FILE)
}

/// Replaces the memory. Empty text removes the file.
pub fn write_in(root: &Path, text: &str) -> Result<(), String> {
    write_file(root, FILE, text)
}

/// The improvements agents proposed, waiting for the user; empty when there are none.
pub fn read_proposals_in(root: &Path) -> Result<String, String> {
    read_file(root, PROPOSALS)
}

/// Replaces the pending proposals. Empty text removes the file.
pub fn write_proposals_in(root: &Path, text: &str) -> Result<(), String> {
    write_file(root, PROPOSALS, text)
}

#[tauri::command]
pub async fn project_proposals_read(root: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || read_proposals_in(Path::new(&root))).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn project_proposals_write(root: String, text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || write_proposals_in(Path::new(&root), &text)).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn project_memory_read(root: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || read_in(Path::new(&root))).await.map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn project_memory_write(root: String, text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || write_in(Path::new(&root), &text)).await.map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("agentdeck-memory-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_project_without_memory_reads_empty_and_a_missing_folder_is_an_error() {
        let folder = temp("empty");
        assert_eq!(read_in(&folder).unwrap(), "");
        assert!(read_in(&folder.join("nope")).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn written_memory_reads_back_and_empty_text_removes_it() {
        let folder = temp("roundtrip");
        write_in(&folder, "- uses pnpm\n- Vietnamese UI copy\n").unwrap();
        assert_eq!(read_in(&folder).unwrap(), "- uses pnpm\n- Vietnamese UI copy\n");
        assert!(folder.join(FILE).is_file());
        write_in(&folder, "  \n").unwrap();
        assert!(!folder.join(FILE).exists());
        write_in(&folder, "").unwrap();
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_runaway_memory_is_refused() {
        let folder = temp("big");
        assert!(write_in(&folder, &"x".repeat(MAX_BYTES + 1)).is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
