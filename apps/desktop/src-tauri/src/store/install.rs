//! Staged jobs: what was fetched for "Add from outside", its saved review, and how a reviewed job is installed.
//! A job lives in `~/.agentdeck/staging/<job-id>`, with `<job-id>.review.json` (the review the user saw),
//! `<job-id>.source` (the git address, when it came from one) and `<job-id>.reply.txt` (Codex's raw review answer).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::library;
use super::review::ReviewVerdict;
use super::scan::{ScanReport, Verdict};
use super::stage;

/// A staged job, as the UI shows it before the review.
#[derive(Serialize, Debug)]
pub struct StagedJob {
    pub job_id: String,
    /// `skill`, `mcp`, `plugin` or `unknown`.
    pub kind: String,
    pub skills: Vec<String>,
    pub scan: ScanReport,
}

/// What the review decided. `verdict` already merges the scan and the review.
#[derive(Serialize, Debug)]
pub struct ReviewOutcome {
    pub verdict: Verdict,
    pub scan: Verdict,
    pub review: Option<ReviewVerdict>,
    pub review_error: Option<String>,
}

/// The review saved next to the job, so the install step checks the same answer the user saw.
#[derive(Serialize, Deserialize, Debug)]
pub struct SavedReview {
    pub review: Option<ReviewVerdict>,
    pub error: Option<String>,
}

pub fn checked_job(job: &str) -> Result<&str, String> {
    if job.is_empty() || !job.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(format!("not a valid job id: {job}"));
    }
    Ok(job)
}

fn job_file(home: &Path, job: &str, suffix: &str) -> PathBuf {
    stage::staging_root(home).join(format!("{job}.{suffix}"))
}

pub fn review_file(home: &Path, job: &str) -> PathBuf {
    job_file(home, job, "review.json")
}

/// Where Codex writes its last message for this job's review. One file per job, so two reviews never mix.
pub fn reply_file(home: &Path, job: &str) -> PathBuf {
    job_file(home, job, "reply.txt")
}

pub fn remember_source(home: &Path, job: &str, url: &str) {
    let _ = std::fs::write(job_file(home, job, "source"), url);
}

/// Where a staged job came from: its git address, or `folder` when the user picked a folder or sent a file.
fn staged_source(home: &Path, job: &str) -> String {
    std::fs::read_to_string(job_file(home, job, "source")).unwrap_or_else(|_| "folder".to_owned())
}

/// Deletes a staged job and every file kept beside it. Called on cancel and after install.
pub fn discard(home: &Path, job: &str) {
    let _ = std::fs::remove_dir_all(stage::staging_root(home).join(job));
    for suffix in ["review.json", "source", "reply.txt"] {
        let _ = std::fs::remove_file(job_file(home, job, suffix));
    }
}

/// The path of a folder inside `root`, with `/` separators (empty for the root itself).
pub fn subpath_within(root: &Path, folder: &Path) -> String {
    folder
        .strip_prefix(root)
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

/// The kind of a staged job, from what it holds.
pub fn kind_of(root: &Path, skills: &[(String, PathBuf)]) -> &'static str {
    if !skills.is_empty() {
        "skill"
    } else if root.join(".claude-plugin").join("plugin.json").is_file() || root.join("plugin.json").is_file() {
        "plugin"
    } else if mcp_config(root).is_some() {
        "mcp"
    } else {
        "unknown"
    }
}

/// The MCP config file of a folder (`mcp.json` or `.mcp.json`), if it has one.
pub fn mcp_config(root: &Path) -> Option<PathBuf> {
    ["mcp.json", ".mcp.json"].into_iter().map(|name| root.join(name)).find(|path| path.is_file())
}

/// Turns an MCP config into one add spec per server, in the shape `mcp_add` reads.
fn mcp_specs(config: &Value, agents: &[String]) -> Vec<Value> {
    let Some(servers) = config["mcpServers"].as_object() else {
        return Vec::new();
    };
    let pairs = |value: &Value, separator: &str| -> Vec<String> {
        value
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(key, value)| format!("{key}{separator}{}", value.as_str().unwrap_or_default()))
                    .collect()
            })
            .unwrap_or_default()
    };
    servers
        .iter()
        .filter_map(|(name, item)| {
            if let Some(url) = item["url"].as_str() {
                return Some(json!({ "name": name, "transport": "http", "target": url, "args": [], "env": [], "headers": pairs(&item["headers"], ": "), "agents": agents }));
            }
            let command = item["command"].as_str()?;
            let args: Vec<String> = item["args"]
                .as_array()
                .map(|list| list.iter().filter_map(|arg| arg.as_str().map(str::to_owned)).collect())
                .unwrap_or_default();
            Some(json!({ "name": name, "transport": "stdio", "target": command, "args": args, "env": pairs(&item["env"], "="), "headers": [], "agents": agents }))
        })
        .collect()
}

/// Adds one spec through `mcp_add`, which runs each agent's own `mcp add`. Returns one line per agent.
pub async fn add_mcp(spec: Value) -> Result<Vec<String>, String> {
    let spec: crate::mcp_manage::AddSpec = serde_json::from_value(spec).map_err(|error| error.to_string())?;
    let results = crate::mcp_manage::mcp_add(spec).await?;
    Ok(results.iter().map(|result| serde_json::to_string(result).unwrap_or_default()).collect())
}

/// Adds every server of an MCP config file for `agents`.
pub async fn add_mcp_file(file: &Path, agents: &[String]) -> Result<Vec<String>, String> {
    let config: Value = std::fs::read_to_string(file)
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))?;
    let mut lines = Vec::new();
    for spec in mcp_specs(&config, agents) {
        lines.extend(add_mcp(spec).await?);
    }
    Ok(lines)
}

/// Installs what a staged job holds: its skills into the library, or its MCP servers. The gate was passed before.
pub async fn install_job(home: &Path, job: &str, root: &Path, agents: &[String]) -> Result<Vec<String>, String> {
    let skills = stage::find_skills(root);
    match kind_of(root, &skills) {
        "skill" => {
            let source = staged_source(home, job);
            let mut lines = Vec::new();
            for (name, path) in &skills {
                library::add(home, name, path, &source, &subpath_within(root, path), agents)?;
                lines.push(format!("{name} installed"));
            }
            Ok(lines)
        }
        "mcp" => match mcp_config(root) {
            Some(file) => add_mcp_file(&file, agents).await,
            None => Err("no MCP config found in this source".to_owned()),
        },
        "plugin" => Err("plugins from outside cannot be installed yet; install them from the agent's marketplace".to_owned()),
        _ => Err("no skill, MCP config or plugin found in this source".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_servers_become_add_specs() {
        let config = json!({ "mcpServers": {
            "files": { "command": "npx", "args": ["-y", "@x/files@1.0.0"], "env": { "ROOT": "/tmp" } },
            "remote": { "url": "https://mcp.example.com/mcp", "headers": { "Authorization": "Bearer t" } }
        }});
        let specs = mcp_specs(&config, &["codex".to_owned()]);
        assert_eq!(specs.len(), 2);
        let files = specs.iter().find(|spec| spec["name"] == "files").expect("stdio server");
        assert_eq!(files["env"][0], "ROOT=/tmp");
        let remote = specs.iter().find(|spec| spec["name"] == "remote").expect("http server");
        assert_eq!(remote["transport"], "http");
        assert_eq!(remote["headers"][0], "Authorization: Bearer t");
    }

    #[test]
    fn job_ids_are_hex_and_dashes_only() {
        assert!(checked_job("18a2f-1234").is_ok());
        assert!(checked_job("../x").is_err());
        assert!(checked_job("").is_err());
    }
}
