//! Rename and delete saved sessions in the tools' own storage.
//!
//! Unlike the readers this writes to `~/.claude` and `~/.codex`, so it stays conservative:
//! - rename only appends one line, the way the tools do themselves;
//! - delete never removes data, it moves the file into `~/.agentdeck/trash` (restorable by hand);
//! - a file touched in the last [`IN_USE`] is refused, a running session would just recreate it.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::json;

use super::{HistoryError, Roots, Source};

/// Longest title accepted; the sidebar and the tools show one line.
const TITLE_MAX: usize = 200;

/// A session file written to more recently than this is probably open in a running CLI.
const IN_USE: Duration = Duration::from_secs(90);

/// `path` canonicalized and proven to live inside the folder of `source`.
fn checked(roots: &Roots, source: Source, path: &Path) -> Result<PathBuf, HistoryError> {
    let root = match source {
        Source::Claude => &roots.claude,
        Source::Codex => &roots.codex,
    };
    let path = path.canonicalize()?;
    if !path.starts_with(root.canonicalize()?) || path.extension().is_none_or(|ext| ext != "jsonl")
    {
        return Err(HistoryError::OutsideRoots);
    }
    Ok(path)
}

fn session_uuid(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_string_lossy();
    match stem.strip_prefix("rollout-") {
        // rollout-<timestamp>-<uuid>: the uuid is the last 36 characters.
        Some(rest) => rest
            .len()
            .checked_sub(36)
            .and_then(|from| rest.get(from..))
            .map(str::to_owned),
        None => Some(stem.into_owned()),
    }
}

/// Gives the session a new title, visible in the tool itself afterwards.
pub fn rename(roots: &Roots, source: Source, path: &Path, title: &str) -> Result<(), HistoryError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(HistoryError::Invalid("the title is empty".into()));
    }
    if title.chars().count() > TITLE_MAX || title.chars().any(char::is_control) {
        return Err(HistoryError::Invalid(format!(
            "the title must be one line of at most {TITLE_MAX} characters"
        )));
    }
    let path = checked(roots, source, path)?;
    let id = session_uuid(&path)
        .ok_or_else(|| HistoryError::Invalid("cannot read the session id".into()))?;
    match source {
        Source::Claude => append(
            &path,
            &json!({"type":"custom-title","customTitle":title,"sessionId":id}),
        ),
        Source::Codex => append(
            &roots.codex_index,
            &json!({"id":id,"thread_name":title,"updated_at":now_iso()}),
        ),
    }
}

/// Moves the session out of the tool's folder into the trash and returns where it went.
pub fn delete(roots: &Roots, source: Source, path: &Path) -> Result<PathBuf, HistoryError> {
    let path = checked(roots, source, path)?;
    let recent = fs::metadata(&path)?
        .modified()
        .ok()
        .and_then(|time| SystemTime::now().duration_since(time).ok())
        .is_some_and(|age| age < IN_USE);
    if recent {
        return Err(HistoryError::InUse);
    }

    let (root, label) = match source {
        Source::Claude => (&roots.claude, "claude"),
        Source::Codex => (&roots.codex, "codex"),
    };
    let relative = path
        .strip_prefix(root.canonicalize()?)
        .map_err(|_| HistoryError::OutsideRoots)?;
    let target = roots.trash.join(label).join(relative);
    relocate(&path, &target)?;

    // Claude keeps subagent and tool output next to the file, in a folder named like it.
    if source == Source::Claude {
        let side = path.with_extension("");
        if side.is_dir() {
            let _ = relocate(&side, &target.with_extension(""));
        }
    }
    Ok(target)
}

fn append(path: &Path, line: &serde_json::Value) -> Result<(), HistoryError> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    // A leading newline keeps the new line whole when the last line was left unterminated.
    writeln!(file, "\n{line}")?;
    Ok(())
}

/// `rename`, falling back to copy + remove when the trash is on another drive.
fn relocate(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    let to = unique(to);
    if fs::rename(from, &to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        copy_dir(from, &to)?;
        fs::remove_dir_all(from)
    } else {
        fs::copy(from, &to)?;
        fs::remove_file(from)
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// A name that does not exist yet, so a second delete never overwrites an earlier one.
fn unique(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{stamp}"));
    path.with_file_name(name)
}

/// Current UTC time as `2026-01-31T12:00:00Z`.
fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_time_has_the_expected_shape() {
        let stamp = now_iso();
        assert_eq!(stamp.len(), 20);
        assert!(stamp.starts_with("20") && stamp.ends_with('Z'));
    }

    #[test]
    fn codex_uuid_is_the_tail_of_the_rollout_name() {
        let path =
            Path::new("rollout-2026-01-01T00-00-00-0198a1b2-c3d4-e5f6-a7b8-c9d0e1f2a3b4.jsonl");
        assert_eq!(
            session_uuid(path).as_deref(),
            Some("0198a1b2-c3d4-e5f6-a7b8-c9d0e1f2a3b4")
        );
    }
}
