// File-explorer helpers beyond rename/trash (those live in lib.rs): creating files and folders, and a file-name
// search over the project. Every path is checked to stay inside the project root.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

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
        // The file search must see the new entry at once, not when its list expires.
        if let Ok(mut cache) = LISTINGS.lock() {
            cache.clear();
        }
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

/// How long the list of a project's files is trusted. Walking a big repository takes a good while; a file made a moment ago
/// shows up in the search once this has passed (the UI also asks for it ahead of time, see `warm`).
const LISTING_TTL: Duration = Duration::from_secs(20);

type Listing = Arc<Vec<String>>;
static LISTINGS: LazyLock<Mutex<HashMap<PathBuf, (Instant, Listing)>>> = LazyLock::new(Default::default);

/// Every file of the project as a path relative to `root`, shortest first: the walk is done once per `LISTING_TTL`.
fn listing(root: &Path) -> Listing {
    if let Ok(cache) = LISTINGS.lock() {
        if let Some((at, files)) = cache.get(root) {
            if at.elapsed() < LISTING_TTL {
                return Arc::clone(files);
            }
        }
    }
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|entry| {
            let name = entry.file_name();
            name != ".git" && name != "node_modules" && name != "target"
        })
        .build();
    let mut files: Vec<String> = walker
        .filter_map(Result::ok)
        .take(200_000)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .map(|entry| relative(root, entry.path()))
        .collect();
    files.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    let files = Arc::new(files);
    if let Ok(mut cache) = LISTINGS.lock() {
        cache.insert(root.to_path_buf(), (Instant::now(), Arc::clone(&files)));
    }
    files
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
        let files = listing(&root);
        // Nothing typed: the shortest paths, which are already first.
        if needle.is_empty() {
            return Ok(files.iter().take(limit).cloned().collect());
        }
        let mut found: Vec<(usize, usize, &str)> = Vec::new();
        for rel in files.iter() {
            let name = rel.rsplit('/').next().unwrap_or(rel);
            // A hit in the file name beats a hit in the folders.
            let score = match fit(name, &needle) {
                Some(at) => at,
                None => match fit(rel, &needle) {
                    Some(at) => 5000 + at,
                    None => continue,
                },
            };
            found.push((score, rel.len(), rel.as_str()));
        }
        found.sort();
        Ok(found
            .into_iter()
            .take(limit)
            .map(|(_, _, rel)| rel.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_list_is_walked_once_shortest_first_and_skips_ignored_folders() {
        let dir = std::env::temp_dir().join(format!("agentdeck-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/deep")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::write(dir.join("a.rs"), "").unwrap();
        std::fs::write(dir.join("src/deep/long_name.rs"), "").unwrap();
        std::fs::write(dir.join("node_modules/x/skip.js"), "").unwrap();
        let root = dir.canonicalize().unwrap();

        let first = listing(&root);
        assert_eq!(first.as_slice(), ["a.rs", "src/deep/long_name.rs"]);
        // A second call inside the trust time is the same list, not a new walk.
        assert!(Arc::ptr_eq(&first, &listing(&root)));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
