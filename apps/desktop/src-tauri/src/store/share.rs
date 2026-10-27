//! Using an installed plugin with the other agents: its skills go into the library (so every agent can read them) and
//! its MCP servers are added for the chosen agents. The plugin folder is scanned first, like any other install.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::install::{add_mcp_file, mcp_config, subpath_within};
use super::library::{self, library_root};
use super::plugins::run;
use super::scan;
use super::stage;

/// Where an installed plugin lives on disk, from the agent's own plugin list.
async fn install_path(agent: &str, id: &str) -> Result<PathBuf, String> {
    let text = run(agent, &["plugin", "list", "--json"]).await?;
    let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    let rows = if agent == "codex" { value["installed"].as_array() } else { value.as_array() };
    rows.and_then(|rows| {
        rows.iter().find_map(|row| {
            let row_id = if agent == "codex" { row["pluginId"].as_str() } else { row["id"].as_str() }?;
            if row_id != id {
                return None;
            }
            let path = if agent == "codex" { row["source"]["path"].as_str() } else { row["installPath"].as_str() }?;
            Some(PathBuf::from(path))
        })
    })
    .ok_or_else(|| format!("{id} is not installed in {agent}"))
}

/// Shares one installed plugin. Returns one line per skill or server.
pub async fn share(home: &Path, agent: &str, id: &str, agents: &[String], acknowledged: bool) -> Result<Vec<String>, String> {
    let folder = install_path(agent, id).await?;
    if !folder.is_dir() {
        return Err(format!("the folder of {id} is no longer on this computer"));
    }
    scan::gate(scan::scan_folder(&folder).verdict, acknowledged, id)?;

    let mut lines = Vec::new();
    for (name, path) in stage::find_skills(&folder) {
        if library_root(home).join(&name).exists() {
            lines.push(format!("{name}: already in the library"));
            continue;
        }
        library::add(home, &name, &path, &format!("plugin:{id}"), &subpath_within(&folder, &path), agents)?;
        lines.push(format!("{name}: skill added"));
    }
    if let Some(file) = mcp_config(&folder) {
        lines.extend(add_mcp_file(&file, agents).await?);
    }
    Ok(lines)
}
