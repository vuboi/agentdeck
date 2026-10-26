//! The Changes panel: what the agent left changed in the project, read from git, so it works for every agent.
//! Lists changed files (staged, unstaged and untracked) with +/- counts, gives one file's unified diff, and
//! reverts or stages files. Paths are relative to the repository top folder.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Change {
    /// Relative to the repository's top folder (what `change_diff`, `change_stage` and `change_revert` take).
    pub path: String,
    /// The same file as a full path: the project folder the chat works in can be a folder inside the repository, so a
    /// path joined onto it would point at a place that does not exist.
    pub abs: String,
    /// `M` modified, `A` added, `D` deleted, `?` untracked.
    pub status: String,
    pub staged: bool,
    pub added: u32,
    pub removed: u32,
}

/// Runs git in `dir` without a console window; `(success, stdout, stderr)`.
fn git(dir: &Path, args: &[&str]) -> Result<(bool, Vec<u8>, String), String> {
    let mut command = Command::new("git");
    command
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command
        .output()
        .map_err(|error| format!("git is not available: {error}"))?;
    Ok((
        output.status.success(),
        output.stdout,
        String::from_utf8_lossy(&output.stderr).trim().to_string(),
    ))
}

fn git_ok(dir: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let (ok, stdout, stderr) = git(dir, args)?;
    if ok {
        Ok(stdout)
    } else {
        Err(if stderr.is_empty() {
            "git failed".to_string()
        } else {
            stderr
        })
    }
}

/// The repository top folder of `root`.
fn toplevel(root: &str) -> Result<PathBuf, String> {
    let (ok, stdout, _) = git(Path::new(root), &["rev-parse", "--show-toplevel"])?;
    if !ok {
        return Err("this folder is not a git repository".to_string());
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&stdout).trim()))
}

/// A path that stays inside the repository: relative, no `..`.
fn checked(path: &str) -> Result<&str, String> {
    let bad = path.is_empty()
        || Path::new(path).is_absolute()
        || path.split(['/', '\\']).any(|part| part == "..");
    if bad {
        Err("path is outside the repository".to_string())
    } else {
        Ok(path)
    }
}

fn has_head(top: &Path) -> bool {
    git(top, &["rev-parse", "--verify", "-q", "HEAD"])
        .map(|result| result.0)
        .unwrap_or(false)
}

/// Lines of an untracked text file (0 for a binary or unreadable one).
fn count_lines(path: &Path) -> u32 {
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() <= 2_000_000 && !bytes.contains(&0) => {
            let lines = bytes.iter().filter(|&&byte| byte == b'\n').count()
                + usize::from(bytes.last().is_some_and(|&byte| byte != b'\n'));
            u32::try_from(lines).unwrap_or(u32::MAX)
        }
        _ => 0,
    }
}

fn list(root: &str) -> Result<Vec<Change>, String> {
    let top = toplevel(root)?;
    let status = git_ok(&top, &["status", "--porcelain=v1", "-z", "-uall"])?;
    let text = String::from_utf8_lossy(&status).into_owned();
    let mut counts = std::collections::HashMap::new();
    if has_head(&top) {
        let numstat = git_ok(&top, &["diff", "HEAD", "--numstat", "-z", "--no-renames"])?;
        for record in String::from_utf8_lossy(&numstat).split('\0') {
            let mut parts = record.splitn(3, '\t');
            if let (Some(added), Some(removed), Some(path)) =
                (parts.next(), parts.next(), parts.next())
            {
                counts.insert(
                    path.to_string(),
                    (
                        added.parse().unwrap_or(0u32),
                        removed.parse().unwrap_or(0u32),
                    ),
                );
            }
        }
    }
    let mut changes = Vec::new();
    let mut entries = text.split('\0').filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (code, path) = entry.split_at(3);
        let mut flags = code.chars();
        let (index, tree) = (flags.next().unwrap_or(' '), flags.next().unwrap_or(' '));
        if matches!(index, 'R' | 'C') {
            entries.next(); // the old name of a rename follows
        }
        let untracked = index == '?';
        let status = if untracked {
            "?"
        } else if index == 'D' || tree == 'D' {
            "D"
        } else if matches!(index, 'A' | 'R' | 'C') {
            "A"
        } else {
            "M"
        };
        let (added, removed) = if untracked {
            (count_lines(&top.join(path)), 0)
        } else {
            counts.get(path).copied().unwrap_or((0, 0))
        };
        changes.push(Change {
            path: path.to_string(),
            abs: top.join(path).to_string_lossy().into_owned(),
            status: status.to_string(),
            staged: !untracked && index != ' ',
            added,
            removed,
        });
    }
    changes.sort_by_key(|change| change.path.to_lowercase());
    Ok(changes)
}

fn diff(root: &str, path: &str) -> Result<String, String> {
    let top = toplevel(root)?;
    let path = checked(path)?;
    let tracked = git(&top, &["ls-files", "--error-unmatch", "--", path])?.0;
    let output = if tracked && has_head(&top) {
        git_ok(
            &top,
            &["diff", "HEAD", "--no-color", "--no-renames", "--", path],
        )?
    } else {
        // Untracked, or no commit yet: diff against nothing. `--no-index` exits 1 when the files differ.
        let null = if cfg!(windows) { "NUL" } else { "/dev/null" };
        git(
            &top,
            &["diff", "--no-index", "--no-color", "--", null, path],
        )?
        .1
    };
    let text = String::from_utf8_lossy(&output).into_owned();
    // Cut git's header (diff --git, index, ---, +++); the panel shows the hunks.
    Ok(match text.find("\n@@") {
        Some(at) => text[at + 1..].to_string(),
        None if text.contains("Binary files") || text.contains("GIT binary patch") => {
            "Binary file".to_string()
        }
        None => text,
    })
}

fn revert_one(top: &Path, path: &str) -> Result<(), String> {
    let path = checked(path)?;
    let in_head = has_head(top) && git(top, &["cat-file", "-e", &format!("HEAD:{path}")])?.0;
    if in_head {
        git_ok(top, &["checkout", "HEAD", "--", path])?;
        return Ok(());
    }
    // Not in the last commit: an added or untracked file is removed.
    if git(top, &["ls-files", "--error-unmatch", "--", path])?.0 {
        git_ok(top, &["rm", "-f", "-q", "--cached", "--", path])?;
    }
    let Ok(canonical) = top.join(path).canonicalize() else {
        return Ok(());
    };
    if !canonical.starts_with(top.canonicalize().map_err(|error| error.to_string())?) {
        return Err("path is outside the repository".to_string());
    }
    std::fs::remove_file(canonical).map_err(|error| error.to_string())
}

/// Reverts many files with a few git calls instead of several per file: the files that are in the last commit come
/// back with one `checkout`, the added ones leave the index with one `rm`, then the files that are not in the commit
/// are deleted. Paths go to git in chunks so the command line stays short enough on Windows.
fn revert_many(top: &Path, changes: &[Change]) -> Result<(), String> {
    const CHUNK: usize = 100;
    let head = has_head(top);
    let (mut restore, mut unstage, mut remove) = (Vec::new(), Vec::new(), Vec::new());
    for change in changes {
        let path = checked(&change.path)?;
        match change.status.as_str() {
            "M" | "D" if head => restore.push(path),
            "?" => remove.push(path),
            _ => {
                unstage.push(path);
                remove.push(path);
            }
        }
    }
    for chunk in restore.chunks(CHUNK) {
        let mut args = vec!["checkout", "HEAD", "--"];
        args.extend(chunk);
        git_ok(top, &args)?;
    }
    for chunk in unstage.chunks(CHUNK) {
        let mut args = vec!["rm", "-f", "-q", "--cached", "--ignore-unmatch", "--"];
        args.extend(chunk);
        git_ok(top, &args)?;
    }
    let canonical_top = top.canonicalize().map_err(|error| error.to_string())?;
    for path in remove {
        let Ok(canonical) = top.join(path).canonicalize() else {
            continue;
        };
        if !canonical.starts_with(&canonical_top) {
            return Err("path is outside the repository".to_string());
        }
        std::fs::remove_file(canonical).map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn changes_list(root: String) -> Result<Vec<Change>, String> {
    blocking(move || list(&root)).await
}

#[tauri::command]
pub async fn change_diff(root: String, path: String) -> Result<String, String> {
    blocking(move || diff(&root, &path)).await
}

/// Puts one file back to the last commit (an added or untracked file is deleted).
#[tauri::command]
pub async fn change_revert(root: String, path: String) -> Result<(), String> {
    blocking(move || revert_one(&toplevel(&root)?, &path)).await
}

/// Puts every changed file back to the last commit. Answers how many were reverted.
#[tauri::command]
pub async fn change_revert_all(root: String) -> Result<usize, String> {
    blocking(move || {
        let top = toplevel(&root)?;
        let changes = list(&root)?;
        revert_many(&top, &changes)?;
        Ok(changes.len())
    })
    .await
}

/// Stages (`stage`) or unstages one file, or every file when `path` is empty.
#[tauri::command]
pub async fn change_stage(root: String, path: String, stage: bool) -> Result<(), String> {
    blocking(move || {
        let top = toplevel(&root)?;
        let target = if path.is_empty() {
            "."
        } else {
            checked(&path)?
        };
        if stage {
            git_ok(&top, &["add", "-A", "--", target])?;
        } else if has_head(&top) {
            git_ok(&top, &["reset", "-q", "HEAD", "--", target])?;
        } else {
            git_ok(&top, &["rm", "-r", "-q", "--cached", "--", target])?;
        }
        Ok(())
    })
    .await
}

/// The local branches of a repository and the one that is checked out.
#[derive(serde::Serialize)]
pub struct Branches {
    /// Empty (None) on a detached HEAD or an unborn branch without commits.
    pub current: Option<String>,
    /// Most recently committed first.
    pub branches: Vec<String>,
}

fn branches(root: &str) -> Result<Branches, String> {
    let top = toplevel(root)?;
    let current = String::from_utf8_lossy(&git_ok(&top, &["branch", "--show-current"])?)
        .trim()
        .to_string();
    let listed = git_ok(
        &top,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/heads",
        ],
    )?;
    let branches = String::from_utf8_lossy(&listed)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    Ok(Branches {
        current: (!current.is_empty()).then_some(current),
        branches,
    })
}

/// A branch name that cannot be read as an option or smuggle in another argument.
fn checked_branch(name: &str) -> Result<&str, String> {
    let bad = name.is_empty()
        || name.starts_with('-')
        || name.chars().any(|c| c.is_whitespace() || c.is_control());
    if bad {
        Err("not a branch name".to_string())
    } else {
        Ok(name)
    }
}

/// The local branches of the project's repository, and the current one.
#[tauri::command]
pub async fn git_branches(root: String) -> Result<Branches, String> {
    blocking(move || branches(&root)).await
}

/// Switches the repository to a local branch. Git's own message comes back when it refuses (local changes in the way).
#[tauri::command]
pub async fn git_switch(root: String, branch: String) -> Result<(), String> {
    blocking(move || {
        let top = toplevel(&root)?;
        let name = checked_branch(&branch)?;
        git_ok(&top, &["switch", name]).map(|_| ())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_names_cannot_be_options() {
        assert!(checked_branch("feat/x").is_ok());
        assert!(checked_branch("--detach").is_err());
        assert!(checked_branch("a b").is_err());
        assert!(checked_branch("").is_err());
    }

    #[test]
    fn paths_must_stay_inside() {
        assert!(checked("src/a.rs").is_ok());
        assert!(checked("../a").is_err());
        assert!(checked("a/../../b").is_err());
        assert!(checked("").is_err());
    }

    #[test]
    fn counts_lines() {
        let file = std::env::temp_dir().join("agentdeck-changes-test.txt");
        std::fs::write(&file, "a\nb\nc").expect("write");
        assert_eq!(count_lines(&file), 3);
        let _ = std::fs::remove_file(file);
    }
}
