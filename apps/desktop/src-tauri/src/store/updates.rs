//! Skill updates: a library skill that came from a git source is compared with that source. A changed skill is scanned
//! again and replaces the library copy only when the scan allows it. The agents keep their links to the library.

use std::path::Path;

use serde::Serialize;

use super::library::{checked_name, copy_tree, library_root};
use super::lock::{self, LockEntry};
use super::scan;
use super::stage::{self, Source};

#[derive(Serialize, Debug)]
pub struct UpdateItem {
    pub name: String,
    pub source: String,
    pub changed: bool,
    pub error: Option<String>,
}

fn from_git(entry: &LockEntry) -> bool {
    entry.source.starts_with("https://")
}

/// Fetches one source into a fresh job and hands its skill folder to `work`. The job is deleted afterwards either way.
async fn with_source<T>(home: &Path, entry: &LockEntry, work: impl FnOnce(&Path) -> Result<T, String>) -> Result<T, String> {
    let job = stage::new_job_id();
    let result = match stage::stage(home, &job, Source::Git { url: entry.source.clone() }).await {
        Ok(root) => work(&root.join(&entry.subpath)),
        Err(error) => Err(error),
    };
    let _ = std::fs::remove_dir_all(stage::staging_root(home).join(&job));
    result
}

/// Stages every git-sourced skill and compares its content with the installed hash.
pub async fn check(home: &Path) -> Vec<UpdateItem> {
    let mut items = Vec::new();
    for (name, entry) in lock::read(home) {
        if !from_git(&entry) {
            continue;
        }
        let result = with_source(home, &entry, |folder| lock::content_hash(folder).map(|hash| hash != entry.hash)).await;
        let (changed, error) = match result {
            Ok(changed) => (changed, None),
            Err(error) => (false, Some(error)),
        };
        items.push(UpdateItem { name, source: entry.source, changed, error });
    }
    items
}

/// Updates one skill from its source. Refused on a block; a caution needs `acknowledged`.
pub async fn apply(home: &Path, name: &str, acknowledged: bool) -> Result<(), String> {
    checked_name(name)?;
    let entry = lock::read(home)
        .remove(name)
        .filter(from_git)
        .ok_or_else(|| format!("{name} has no git source to update from"))?;
    with_source(home, &entry, |folder| {
        if !folder.join("SKILL.md").is_file() {
            return Err(format!("the source no longer has {name}'s SKILL.md where it was"));
        }
        scan::gate(scan::scan_folder(folder).verdict, acknowledged, &format!("the new version of {name}"))?;
        replace(home, name, folder, &entry)
    })
    .await
}

/// Prepare a complete replacement first. Swap folders so deleted upstream files cannot survive an update.
fn replace(home: &Path, name: &str, folder: &Path, entry: &LockEntry) -> Result<(), String> {
    checked_name(name)?;
    let target = library_root(home).join(name);
    let work = home.join(".agentdeck/library/.updates").join(stage::new_job_id());
    let prepared = work.join("new");
    let backup = work.join("old");
    let result = (|| {
        copy_tree(folder, &prepared)?;
        if !prepared.join("SKILL.md").is_file() { return Err("Replacement has no SKILL.md".into()); }
        let had_old = target.exists();
        if had_old { std::fs::rename(&target, &backup).map_err(|e| e.to_string())?; }
        if let Err(error) = std::fs::rename(&prepared, &target) {
            if had_old { std::fs::rename(&backup, &target).map_err(|e| format!("Replacement failed: {error}; recovery failed: {e}. Old skill retained at {}", backup.display()))?; }
            return Err(error.to_string());
        }
        if let Err(error) = lock::record(home, name, &entry.source, &entry.subpath, &target) {
            std::fs::remove_dir_all(&target).map_err(|e| format!("{error}; could not remove incomplete replacement: {e}"))?;
            if had_old { std::fs::rename(&backup, &target).map_err(|e| format!("{error}; recovery failed: {e}. Old skill retained at {}", backup.display()))?; }
            return Err(error);
        }
        Ok(())
    })();
    // A failed rollback leaves the backup available for recovery instead of deleting it.
    if result.is_ok() || !backup.exists() { let _ = std::fs::remove_dir_all(&work); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_removes_obsolete_files_and_preparation_failure_preserves_old_skill() {
        let home = std::env::temp_dir().join(format!("agentdeck-replace-{}", stage::new_job_id()));
        let old = library_root(&home).join("example");
        let source = home.join("incoming");
        std::fs::create_dir_all(&old).expect("old folder");
        std::fs::create_dir_all(&source).expect("new folder");
        std::fs::write(old.join("SKILL.md"), "old").expect("old skill");
        std::fs::write(old.join("obsolete.txt"), "obsolete").expect("old extra file");
        let entry = LockEntry { source: "https://example.org/skill".into(), subpath: String::new(), hash: String::new(), installed_at: 0 };
        assert!(replace(&home, "example", &source, &entry).is_err());
        assert_eq!(std::fs::read_to_string(old.join("SKILL.md")).expect("preserved"), "old");
        std::fs::write(source.join("SKILL.md"), "new").expect("new skill");
        replace(&home, "example", &source, &entry).expect("replace");
        assert_eq!(std::fs::read_to_string(old.join("SKILL.md")).expect("updated"), "new");
        assert!(!old.join("obsolete.txt").exists());
        assert_eq!(lock::read(&home)["example"].hash, lock::content_hash(&old).expect("hash"));
        std::fs::remove_dir_all(home).expect("cleanup");
    }
}
