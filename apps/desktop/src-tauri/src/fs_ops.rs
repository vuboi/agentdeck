// File-explorer helpers beyond rename/trash (those live in lib.rs): creating files and folders, and a file-name
// search over the project. Every path is checked to stay inside the project root.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

fn valid_name(name: &str) -> Result<String, String> {
    let name = name.trim().to_string();
    let bad = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    if name.is_empty() || name == "." || name == ".." || name.contains(bad) {
        return Err(format!("\u{201c}{name}\u{201d} is not a valid name"));
    }
    Ok(name)
}

/// Creates an empty file (or a folder) called `name` inside `dir`, a folder of the project. Answers the new path.
#[tauri::command]
pub async fn path_create(
    root: String,
    dir: String,
    name: String,
    folder: bool,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let name = valid_name(&name)?;
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let dir = PathBuf::from(&dir)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !dir.starts_with(&root) || !dir.is_dir() {
            return Err("that is outside the project folder".to_string());
        }
        let target = dir.join(&name);
        if target.exists() {
            return Err(format!("{name} already exists here"));
        }
        if folder {
            std::fs::create_dir(&target)
        } else {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .map(|_| ())
        }
        .map_err(|error| error.to_string())?;
        Ok(target.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| error.to_string())?
}

fn fit(hay: &str, needle: &str) -> Option<usize> {
    let hay = hay.to_lowercase();
    if let Some(at) = hay.find(needle) {
        return Some(at);
    }
    // The letters in order ("hdr" finds "header").
    let mut from = 0;
    for ch in needle.chars() {
        let next = hay[from..].find(ch)?;
        from += next + ch.len_utf8();
    }
    Some(1000 + from)
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Files of the project whose path matches `query` (best first, at most `limit`), as paths relative to `root`
/// with `/` separators. Honours .gitignore; `.git`, `node_modules` and `target` are skipped.
#[tauri::command]
pub async fn project_files(
    root: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let needle = query.trim().to_lowercase().replace('\\', "/");
        let limit = limit.unwrap_or(50).clamp(1, 500);
        let mut found: Vec<(usize, usize, String)> = Vec::new();
        let walker = WalkBuilder::new(&root)
            .hidden(false)
            .filter_entry(|entry| {
                let name = entry.file_name();
                name != ".git" && name != "node_modules" && name != "target"
            })
            .build();
        for entry in walker.filter_map(Result::ok).take(200_000) {
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                continue;
            }
            let rel = relative(&root, entry.path());
            if needle.is_empty() {
                found.push((0, rel.len(), rel));
                continue;
            }
            let name = rel.rsplit('/').next().unwrap_or(&rel).to_string();
            // A hit in the file name beats a hit in the folders.
            let score = match fit(&name, &needle) {
                Some(at) => at,
                None => match fit(&rel, &needle) {
                    Some(at) => 5000 + at,
                    None => continue,
                },
            };
            found.push((score, rel.len(), rel));
        }
        found.sort();
        Ok(found
            .into_iter()
            .take(limit)
            .map(|(_, _, rel)| rel)
            .collect())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Writes `text` to an existing file of the project (the file viewer's editor). The file must sit inside `root`;
/// the bytes go to a temporary file beside it first and replace it in one step, so a crash never leaves half a file.
#[tauri::command]
pub async fn file_write(root: String, path: String, text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&root) || !path.is_file() {
            return Err("that file is outside the project folder".to_string());
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let temp = path.with_file_name(format!(".{name}.agentdeck-save"));
        let written = (|| {
            std::fs::write(&temp, text.as_bytes())?;
            if let Ok(meta) = std::fs::metadata(&path) {
                let _ = std::fs::set_permissions(&temp, meta.permissions());
            }
            std::fs::rename(&temp, &path)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        written.map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}
