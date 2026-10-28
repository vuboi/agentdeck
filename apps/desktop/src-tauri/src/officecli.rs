//! OfficeCLI (github.com/iOfficeAI/OfficeCLI, Apache 2.0): one binary that creates, reads and edits Word, Excel and PowerPoint
//! files for an agent, with no Office installed. AgentDeck does not bundle it: the first install downloads the release for
//! this system into the app's data folder, checks that it runs, registers it as an MCP server (`officecli mcp`) with the
//! agents the user picks, and installs its skill (SKILL.md) through the Store's path.
//!
//! The binary's file name per system is in `asset_for`; the platforms without a release (and so without a build here) are
//! reported as unsupported.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// Where the release files are. `latest` moves with each release; the file names are the ones the release publishes.
const RELEASE: &str = "https://github.com/iOfficeAI/OfficeCLI/releases/latest/download";
const REPO: &str = "https://github.com/iOfficeAI/OfficeCLI";
/// The MCP server name the agents see.
const SERVER: &str = "officecli";
/// Largest download accepted: the release binary is far below this; the cap only stops a runaway transfer.
const MAX_BYTES: u64 = 300 * 1024 * 1024;
/// Agents that take an MCP server from AgentDeck (see mcp_manage.rs `add_args`).
const AGENTS: &[&str] = &["claude", "codex", "antigravity", "copilot"];

/// The release file of OfficeCLI for an operating system and CPU (`std::env::consts` names), or None.
pub fn asset_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("officecli-win-x64.exe"),
        ("windows", "aarch64") => Some("officecli-win-arm64.exe"),
        ("macos", "aarch64") => Some("officecli-mac-arm64"),
        ("macos", "x86_64") => Some("officecli-mac-x64"),
        ("linux", "x86_64") => Some("officecli-linux-x64"),
        ("linux", "aarch64") => Some("officecli-linux-arm64"),
        _ => None,
    }
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|error| error.to_string())?.join("officecli"))
}

fn binary_in(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) { "officecli.exe" } else { "officecli" })
}

/// Runs the binary with `--version` and returns what it prints, or None when it does not start.
fn version_of(binary: &Path) -> Option<String> {
    let mut command = std::process::Command::new(binary);
    command.arg("--version").stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

#[derive(Serialize)]
pub struct Status {
    /// This system has an OfficeCLI release.
    supported: bool,
    /// The binary is in the app's folder and runs.
    installed: bool,
    version: Option<String>,
}

#[tauri::command]
pub async fn officecli_status(app: AppHandle) -> Result<Status, String> {
    let root = root(&app)?;
    let supported = asset_for(std::env::consts::OS, std::env::consts::ARCH).is_some();
    let binary = binary_in(&root);
    tauri::async_runtime::spawn_blocking(move || {
        let version = if binary.is_file() { version_of(&binary) } else { None };
        Status { supported, installed: version.is_some(), version }
    })
    .await
    .map_err(|error| error.to_string())
}

/// Downloads the release file to `dest` (through a `.part` file, so a cut transfer leaves nothing that looks installed).
async fn download(url: &str, dest: &Path) -> Result<(), String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    if let Some(folder) = dest.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let client = reqwest::Client::builder()
        .user_agent("AgentDeck")
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client.get(url).send().await.map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("download failed ({}): {url}", response.status()));
    }
    if response.content_length().is_some_and(|length| length > MAX_BYTES) {
        return Err("the OfficeCLI download is larger than expected".into());
    }
    let part = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&part).await.map_err(|error| error.to_string())?;
    let mut stream = response.bytes_stream();
    let mut total = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        total += chunk.len() as u64;
        if total > MAX_BYTES {
            let _ = tokio::fs::remove_file(&part).await;
            return Err("the OfficeCLI download is larger than expected".into());
        }
        file.write_all(&chunk).await.map_err(|error| error.to_string())?;
    }
    file.flush().await.map_err(|error| error.to_string())?;
    drop(file);
    std::fs::rename(&part, dest).map_err(|error| error.to_string())
}

#[derive(Serialize)]
pub struct Setup {
    version: String,
    /// What each agent said about the MCP server (the same shape as the MCP settings).
    agents: Vec<crate::mcp_manage::AgentResult>,
    /// The skill's result, or why it was not installed (a scan flag needs the Store).
    skill: String,
}

/// Downloads OfficeCLI if it is not there, registers its MCP server with the agents and installs its skill. Safe to run
/// again: an existing binary is kept only if it still runs, and the MCP entry is replaced.
#[tauri::command]
pub async fn officecli_install(app: AppHandle) -> Result<Setup, String> {
    // One install at a time: two would write the same download file.
    if INSTALLING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err("OfficeCLI is already being installed".into());
    }
    let result = install(&app).await;
    INSTALLING.store(false, std::sync::atomic::Ordering::SeqCst);
    result
}

static INSTALLING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

async fn install(app: &AppHandle) -> Result<Setup, String> {
    let asset = asset_for(std::env::consts::OS, std::env::consts::ARCH)
        .ok_or("OfficeCLI has no release for this system")?;
    let root = root(app)?;
    let binary = binary_in(&root);
    // An existing binary is kept only if it still runs; otherwise it is downloaded again.
    let existing = if binary.is_file() {
        let probe = binary.clone();
        tauri::async_runtime::spawn_blocking(move || version_of(&probe))
            .await
            .map_err(|error| error.to_string())?
    } else {
        None
    };
    let version = match existing {
        Some(version) => version,
        None => {
            download(&format!("{RELEASE}/{asset}"), &binary).await?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).map_err(|error| error.to_string())?;
            }
            let probe = binary.clone();
            tauri::async_runtime::spawn_blocking(move || version_of(&probe))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("the downloaded OfficeCLI does not run on this system")?
        }
    };

    // MCP: replace an earlier entry, then add it for each agent with its own command (mcp_manage keeps those per agent).
    let path = binary.to_string_lossy().into_owned();
    let agents: Vec<String> = AGENTS.iter().map(|agent| (*agent).to_owned()).collect();
    let _ = crate::mcp_manage::mcp_remove(SERVER.to_owned(), agents.clone(), None).await;
    let spec = serde_json::from_value::<crate::mcp_manage::AddSpec>(serde_json::json!({
        "name": SERVER,
        "transport": "stdio",
        "target": path,
        "args": ["mcp"],
        "agents": agents,
    }))
    .map_err(|error| error.to_string())?;
    let results = crate::mcp_manage::mcp_add(spec).await?;

    let skill = match crate::store::add_skill_from_git(REPO, "").await {
        Ok(messages) => messages.join(", "),
        Err(error) => format!("skill not installed: {error}"),
    };
    Ok(Setup { version, agents: results, skill })
}

/// Removes the binary and its MCP entries. The skill stays in the Store (remove it there).
#[tauri::command]
pub async fn officecli_remove(app: AppHandle) -> Result<(), String> {
    let agents: Vec<String> = AGENTS.iter().map(|agent| (*agent).to_owned()).collect();
    let _ = crate::mcp_manage::mcp_remove(SERVER.to_owned(), agents, None).await;
    let root = root(&app)?;
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_system_has_its_release_file() {
        assert_eq!(asset_for("windows", "x86_64"), Some("officecli-win-x64.exe"));
        assert_eq!(asset_for("windows", "aarch64"), Some("officecli-win-arm64.exe"));
        assert_eq!(asset_for("macos", "aarch64"), Some("officecli-mac-arm64"));
        assert_eq!(asset_for("macos", "x86_64"), Some("officecli-mac-x64"));
        assert_eq!(asset_for("linux", "x86_64"), Some("officecli-linux-x64"));
        assert_eq!(asset_for("linux", "aarch64"), Some("officecli-linux-arm64"));
        assert_eq!(asset_for("freebsd", "x86_64"), None);
    }

    #[test]
    fn a_missing_binary_does_not_run() {
        assert_eq!(version_of(Path::new("/definitely/not/officecli")), None);
    }
}
