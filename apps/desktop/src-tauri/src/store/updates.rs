//! Skill updates: a library skill that came from a git source is compared with that source. A changed skill is scanned
//! again and replaces the library copy only when the scan allows it. The agents keep their links to the library.

use std::path::Path;

use serde::Serialize;

use super::library::{copy_tree, library_root};
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
    let entry = lock::read(home)
        .remove(name)
        .filter(from_git)
        .ok_or_else(|| format!("{name} has no git source to update from"))?;
    with_source(home, &entry, |folder| {
        if !folder.join("SKILL.md").is_file() {
            return Err(format!("the source no longer has {name}'s SKILL.md where it was"));
        }
        scan::gate(scan::scan_folder(folder).verdict, acknowledged, &format!("the new version of {name}"))?;
        let target = library_root(home).join(name);
        if target.exists() {
            std::fs::remove_dir_all(&target).map_err(|error| error.to_string())?;
        }
        copy_tree(folder, &target)?;
        lock::record(home, name, &entry.source, &entry.subpath, &target)
    })
    .await
}
