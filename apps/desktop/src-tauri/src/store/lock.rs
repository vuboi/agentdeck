//! The library's lock file: for every skill in the library, where it came from and the content hash it was installed with.
//! The update check compares the source's current content with this hash, so a skill is only updated when it changed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LockEntry {
    /// The git address the skill came from, `plugin:<id>`, or `folder` for a folder the user picked.
    pub source: String,
    /// Where the skill sits inside the source (empty when it is the source root).
    #[serde(default)]
    pub subpath: String,
    /// Hash of the skill's files when it was installed.
    pub hash: String,
    pub installed_at: u64,
}

pub type Lock = BTreeMap<String, LockEntry>;

fn lock_file(home: &Path) -> PathBuf {
    home.join(".agentdeck").join("library").join("lock.json")
}

pub fn read(home: &Path) -> Lock {
    std::fs::read_to_string(lock_file(home))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write(home: &Path, lock: &Lock) -> Result<(), String> {
    let file = lock_file(home);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = file.with_extension(format!("{}.tmp", super::stage::new_job_id()));
    let result = (|| {
        std::fs::write(&temporary, serde_json::to_string_pretty(lock).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
        std::fs::rename(&temporary, &file).map_err(|error| error.to_string())
    })();
    if result.is_err() { let _ = std::fs::remove_file(&temporary); }
    result
}

/// Records one installed skill. `folder` is the skill's folder as it was installed (its files are hashed now).
pub fn record(home: &Path, name: &str, source: &str, subpath: &str, folder: &Path) -> Result<(), String> {
    let mut lock = read(home);
    let installed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    lock.insert(
        name.to_owned(),
        LockEntry {
            source: source.to_owned(),
            subpath: subpath.to_owned(),
            hash: content_hash(folder)?,
            installed_at,
        },
    );
    write(home, &lock)
}

pub fn forget(home: &Path, name: &str) -> Result<(), String> {
    let mut lock = read(home);
    lock.remove(name);
    write(home, &lock)
}

/// Hash of a folder: the relative path and the bytes of every file, in path order. Links and `.git` are left out.
pub fn content_hash(folder: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    collect(folder, folder, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(std::fs::read(&path).map_err(|error| error.to_string())?);
        hasher.update([0]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|error| error.to_string())?.flatten() {
        let path = entry.path();
        if std::fs::read_link(&path).is_ok() {
            continue;
        }
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        if entry.file_type().map_err(|error| error.to_string())?.is_dir() {
            collect(root, &path, out)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map(|rel| rel.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            out.push((relative, path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_changes_when_a_file_changes_and_not_otherwise() {
        let dir = std::env::temp_dir().join(format!("agentdeck-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("makes");
        std::fs::write(dir.join("SKILL.md"), "one").expect("writes");
        let first = content_hash(&dir).expect("hashes");
        assert_eq!(first, content_hash(&dir).expect("hashes again"));
        std::fs::write(dir.join("SKILL.md"), "two").expect("writes");
        assert_ne!(first, content_hash(&dir).expect("hashes"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
