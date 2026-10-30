//! Brings an outside skill, MCP config or plugin into staging, where it is scanned and reviewed. Nothing here runs the
//! content: git is cloned without hooks, zips are extracted without their links, and downloads are kept as text.
//!
//! A staged job is a folder `~/.agentdeck/staging/<job-id>`. It is deleted when the install finishes or is cancelled.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

const MAX_DOWNLOAD: usize = 1024 * 1024;
const MAX_ARCHIVE: u64 = 50 * 1024 * 1024;

/// Where the outside content comes from. The UI sends one of these.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    /// A folder the user picked.
    Folder { path: String },
    /// A git repository over https.
    Git { url: String },
    /// A `.zip` file on this computer.
    Zip { path: String },
    /// One file served over https: a `SKILL.md` or an MCP config JSON.
    Url { url: String },
}

pub fn staging_root(home: &Path) -> PathBuf {
    home.join(crate::channel::home_folder_name()).join("staging")
}

/// A new job id: the time in hex plus the process id, unique enough for one user's machine.
pub fn new_job_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("{now:x}-{}", std::process::id())
}

/// Rejects anything that is not an https address. A leading `-` would read as a git option, so it is refused too.
pub fn checked_https(url: &str) -> Result<&str, String> {
    let url = url.trim();
    if !url.starts_with("https://") || url.contains(char::is_whitespace) {
        return Err("only an https:// address without spaces is accepted".to_owned());
    }
    Ok(url)
}

/// Writes the content into `dest` (created empty). Returns the folder to scan.
pub async fn stage(home: &Path, job: &str, source: Source) -> Result<PathBuf, String> {
    let dest = staging_root(home).join(job);
    if dest.exists() {
        return Err(format!("job {job} already exists"));
    }
    std::fs::create_dir_all(&dest).map_err(|error| error.to_string())?;
    let result = match source {
        Source::Folder { path } => copy_folder(Path::new(&path), &dest),
        Source::Git { url } => clone(checked_https(&url)?, &dest).await,
        Source::Zip { path } => extract_zip(Path::new(&path), &dest).await,
        Source::Url { url } => download(checked_https(&url)?, &dest).await,
    };
    if let Err(error) = result {
        let _ = std::fs::remove_dir_all(&dest);
        return Err(error);
    }
    Ok(dest)
}

fn copy_folder(from: &Path, to: &Path) -> Result<(), String> {
    if !from.is_dir() {
        return Err("not a folder".to_owned());
    }
    super::library::copy_tree(from, to)
}

/// Clone with hooks off and no submodules. The `.git` folder is removed afterwards so nothing from it is kept.
async fn clone(url: &str, dest: &Path) -> Result<(), String> {
    let hooks = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut command = tokio::process::Command::new("git");
    command
        .args(["-c", &format!("core.hooksPath={hooks}"), "clone", "--depth", "1", "--no-recurse-submodules", "--single-branch", "--", url])
        .arg(dest.join("repo"))
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null());
    let output = tokio::time::timeout(Duration::from_secs(180), command.output())
        .await
        .map_err(|_| "git clone took more than 180 seconds".to_owned())?
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    // The clone sits in its own folder so the job root is the repository content.
    let repo = dest.join("repo");
    let _ = std::fs::remove_dir_all(repo.join(".git"));
    for entry in std::fs::read_dir(&repo).map_err(|error| error.to_string())?.flatten() {
        std::fs::rename(entry.path(), dest.join(entry.file_name())).map_err(|error| error.to_string())?;
    }
    std::fs::remove_dir_all(&repo).map_err(|error| error.to_string())
}

/// One git command with hooks turned off and no prompts. Fails with git's own error text.
async fn git_no_hooks(args: &[&str]) -> Result<(), String> {
    let hooks = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut command = tokio::process::Command::new("git");
    command
        .args(["-c", &format!("core.hooksPath={hooks}")])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null());
    let output = tokio::time::timeout(Duration::from_secs(300), command.output())
        .await
        .map_err(|_| "git took more than 300 seconds".to_owned())?
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// Clones `url` without hooks and checks out the pinned commit `sha`. The clone's content is moved into `dest`.
/// Used for plugin sources, so what is scanned is exactly what the CLI will install.
pub async fn clone_at(url: &str, sha: &str, dest: &Path) -> Result<(), String> {
    if sha.len() < 7 || sha.len() > 64 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("the plugin commit is not a valid sha".to_owned());
    }
    let url = checked_https(url)?;
    let repo = dest.join("repo");
    let repo_arg = repo.to_string_lossy().into_owned();
    git_no_hooks(&["clone", "--filter=blob:none", "--no-checkout", "--", url, &repo_arg]).await?;
    git_no_hooks(&["-C", &repo_arg, "checkout", "--detach", sha]).await?;
    let _ = std::fs::remove_dir_all(repo.join(".git"));
    for entry in std::fs::read_dir(&repo).map_err(|error| error.to_string())?.flatten() {
        std::fs::rename(entry.path(), dest.join(entry.file_name())).map_err(|error| error.to_string())?;
    }
    std::fs::remove_dir_all(&repo).map_err(|error| error.to_string())
}

/// Lists the archive first and refuses absolute paths and `..`, then extracts it with `tar` (which reads zip too).
async fn extract_zip(path: &Path, dest: &Path) -> Result<(), String> {
    let size = std::fs::metadata(path).map(|meta| meta.len()).map_err(|error| error.to_string())?;
    if size > MAX_ARCHIVE {
        return Err("the zip file is over 50 MB".to_owned());
    }
    let listing = tokio::process::Command::new("tar")
        .arg("-tf")
        .arg(path)
        .output()
        .await
        .map_err(|error| error.to_string())?;
    if !listing.status.success() {
        return Err("the zip file cannot be read".to_owned());
    }
    for line in String::from_utf8_lossy(&listing.stdout).lines() {
        let entry = line.trim();
        if entry.starts_with('/') || entry.starts_with('\\') || entry.contains(':') || entry.split(['/', '\\']).any(|part| part == "..") {
            return Err(format!("the zip has an unsafe path: {entry}"));
        }
    }
    let output = tokio::process::Command::new("tar")
        .arg("-xf")
        .arg(path)
        .arg("-C")
        .arg(dest)
        .output()
        .await
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(())
}

/// One file over https. Kept as text: a `.md` becomes `SKILL.md`, anything else `mcp.json`.
async fn download(url: &str, dest: &Path) -> Result<(), String> {
    let response = reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("download failed: HTTP {}", response.status()));
    }
    let body = response.bytes().await.map_err(|error| error.to_string())?;
    if body.len() > MAX_DOWNLOAD {
        return Err("the download is over 1 MB".to_owned());
    }
    let name = if url.split('?').next().unwrap_or(url).to_lowercase().ends_with(".md") {
        "SKILL.md"
    } else {
        "mcp.json"
    };
    std::fs::write(dest.join(name), &body).map_err(|error| error.to_string())
}

/// Skill folders in a staged job, including provider and plugin subfolders. Do not follow links or
/// descend into a skill's bundled resources, dependency trees, build output or test fixtures.
pub fn find_skills(root: &Path) -> Vec<(String, PathBuf)> {
    let mut found = Vec::new();
    let mut entries = walkdir::WalkDir::new(root).follow_links(false).max_depth(12).into_iter();
    while let Some(entry) = entries.next() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_dir() { continue; }
        let name = entry.file_name().to_string_lossy();
        if entry.depth() > 0 && matches!(name.as_ref(), ".git" | "node_modules" | "target" | "dist" | "build" | ".venv" | "venv" | "__pycache__" | "test" | "tests" | "fixtures") {
            entries.skip_current_dir();
            continue;
        }
        let manifest = entry.path().join("SKILL.md");
        if manifest.is_file() && std::fs::read_link(&manifest).is_err() {
            found.push((name.into_owned(), entry.path().to_path_buf()));
            entries.skip_current_dir();
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_is_accepted() {
        assert!(checked_https("https://github.com/x/y").is_ok());
        assert!(checked_https("http://example.com/a").is_err());
        assert!(checked_https("--upload-pack=evil").is_err());
        assert!(checked_https("https://a b").is_err());
    }

    #[test]
    fn skills_are_found_at_root_or_one_level_down() {
        let dir = std::env::temp_dir().join(format!("agentdeck-stage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for name in ["alpha", "beta"] {
            std::fs::create_dir_all(dir.join("skills").join(name)).expect("makes");
            std::fs::write(dir.join("skills").join(name).join("SKILL.md"), "x").expect("writes");
        }
        let found: Vec<String> = find_skills(&dir).into_iter().map(|(name, _)| name).collect();
        assert_eq!(found, vec!["alpha".to_owned(), "beta".to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nested_provider_and_plugin_skills_exclude_fixtures_and_bundled_skills() {
        let dir = std::env::temp_dir().join(format!("agentdeck-nested-skills-{}", new_job_id()));
        for folder in [".agents/skills/review-pr", "plugins/official/web/skills/web", "tests/fixtures/fixture", "node_modules/dependency", ".agents/skills/review-pr/resources/example"] {
            std::fs::create_dir_all(dir.join(folder)).expect("creates folder");
            std::fs::write(dir.join(folder).join("SKILL.md"), "skill").expect("writes manifest");
        }
        let found = find_skills(&dir);
        assert_eq!(found.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["review-pr", "web"]);
        assert!(found.iter().all(|(_, path)| path.starts_with(&dir)));
        std::fs::write(dir.join("SKILL.md"), "root skill").expect("writes root manifest");
        assert_eq!(find_skills(&dir).len(), 1);
        std::fs::remove_dir_all(&dir).expect("cleans fixture");
    }
}
