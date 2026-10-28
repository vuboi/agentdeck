//! Explicit Store synchronization. Existing independent folders and conflicting MCP entries are preserved.
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use super::{library, updates};

#[derive(Deserialize)]
pub struct SyncRequest {
    /// Empty means user/global scope; otherwise only these project folders are changed.
    projects: Vec<PathBuf>,
    /// Omitted names synchronize every managed library skill and discovered user MCP server.
    name: Option<String>,
    update: bool,
}

#[derive(Serialize)]
pub struct SyncResult { pub target: String, pub item: String, pub ok: bool, pub message: String }

/// Applies updates and links/config merges, returning each failure without hiding successful targets.
#[tauri::command]
pub async fn store_sync(request: SyncRequest) -> Result<Vec<SyncResult>, String> {
    let home = crate::home_folder()?;
    remember_projects(&home, &request.projects)?;
    let mut results = Vec::new();
    let skills = library::installed_skills(&home).into_iter().filter(|s| s.origin == "library")
        .filter(|s| request.name.as_ref().is_none_or(|n| n == &s.name)).collect::<Vec<_>>();
    for skill in skills {
        if request.update {
            if !super::lock::read(&home).get(&skill.name).is_some_and(|e| e.source.starts_with("https://")) {
                record(&mut results, "library", &skill.name, Err("No Git source recorded; cannot download a replacement. Use Sync all without updating to sync existing files.".into()));
                continue;
            }
            let updated = updates::apply(&home, &skill.name, false).await;
            let failed = updated.is_err();
            record(&mut results, "library", &skill.name, updated);
            if failed { continue; }
        }
        if request.projects.is_empty() {
            let agents = library::SKILL_AGENTS.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
            record(&mut results, "global", &skill.name, library::assign(&home, &skill.name, &agents));
        } else {
            for project in &request.projects {
                for folder in library::project_folders() {
                    let result = link_skill(project, folder, &skill.name, &library::library_root(&home).join(&skill.name));
                    record(&mut results, &format!("{} / {folder}", project.display()), &skill.name, result);
                }
            }
        }
    }
    results.extend(crate::mcp_manage::sync_store(&request.projects, request.name.as_deref()).await);
    Ok(results)
}

fn record(results: &mut Vec<SyncResult>, target: &str, item: &str, result: Result<(), String>) {
    results.push(SyncResult { target: target.into(), item: item.into(), ok: result.is_ok(), message: result.err().unwrap_or_else(|| "Synchronized".into()) });
}

/// Installs a catalog MCP definition directly at project scope without enabling it globally.
pub(super) fn install_project_mcp(project: &Path, spec: Value) -> Result<Vec<String>, String> {
    remember_projects(&crate::home_folder()?, &[project.to_path_buf()])?;
    let name = spec["name"].as_str().ok_or("MCP name is missing")?;
    library::checked_name(name)?;
    let target = spec["target"].as_str().ok_or("MCP target is missing")?;
    let http = spec["transport"] == "http";
    let mut lines = Vec::new();
    for (file, key, is_toml) in [(".mcp.json", "mcpServers", false), (".codex/config.toml", "mcp_servers", true), (".copilot/mcp-config.json", "mcpServers", false)] {
        let mut server = if http { json!({"url":target}) } else { json!({"command":target,"args":spec["args"].as_array().cloned().unwrap_or_default()}) };
        if !is_toml { server["type"] = json!(if http { "http" } else { "stdio" }); }
        for (input, output, separator) in [("env", "env", '='), ("headers", if is_toml {"http_headers"} else {"headers"}, ':')] {
            let mut values = serde_json::Map::new();
            if let Some(entries) = spec[input].as_array() {
                for entry in entries {
                    let (key, value) = entry.as_str().and_then(|s| s.split_once(separator)).ok_or("Invalid MCP environment/header pair")?;
                    values.insert(key.trim().into(), json!(value.trim()));
                }
            }
            if !values.is_empty() { server[output] = Value::Object(values); }
        }
        match merge_mcp(&project.join(file), key, name, server, is_toml) {
            Ok(()) => lines.push(format!("{name}: {file} installed")),
            Err(error) => return Err(format!("{file}: {error}. Earlier targets may already be installed.")),
        }
    }
    Ok(lines)
}

pub(super) fn link_skill(project: &Path, folder: &str, name: &str, source: &Path) -> Result<(), String> {
    library::checked_name(name)?;
    if !project.is_absolute() || !project.is_dir() { return Err("Project must be an existing absolute folder".into()); }
    let folder = project.join(folder);
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let target = folder.join(name);
    if target.symlink_metadata().is_ok() {
        if std::fs::canonicalize(&target).ok() == std::fs::canonicalize(source).ok() { return Ok(()); }
        return Err("Existing independent skill folder preserved".into());
    }
    library::link_dir(&target, source)
}

/// Merges one server into a provider config. Divergent entries require a manual resolution.
pub(crate) fn merge_mcp(path: &Path, key: &str, name: &str, server: Value, toml_file: bool) -> Result<(), String> {
    let mut root = match std::fs::read_to_string(path) {
        Ok(text) if toml_file => serde_json::to_value(toml::from_str::<toml::Value>(&text).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?,
        Ok(text) => serde_json::from_str(&text).map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(e.to_string()),
    };
    let object = root.as_object_mut().ok_or("Config root must be an object")?;
    let servers = object.entry(key).or_insert_with(|| json!({})).as_object_mut().ok_or("Invalid MCP server map")?;
    if let Some(old) = servers.get(name) {
        return if old == &server { Ok(()) } else { Err("Different existing MCP configuration preserved".into()) };
    }
    servers.insert(name.to_owned(), server);
    let text = if toml_file { toml::to_string_pretty(&root).map_err(|e| e.to_string())? } else { serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? };
    let parent = path.parent().ok_or("Config has no parent")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Project roots explicitly selected in Store; used only to discover managed installations again.
pub(crate) fn known_projects(home: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(home.join(".agentdeck/store-projects.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub(super) fn remember_projects(home: &Path, projects: &[PathBuf]) -> Result<(), String> {
    if projects.is_empty() { return Ok(()); }
    let mut known = known_projects(home);
    for project in projects {
        if !project.is_absolute() || !project.is_dir() { return Err("Project must be an existing absolute folder".into()); }
        if !known.contains(project) { known.push(project.clone()); }
    }
    let file = home.join(".agentdeck/store-projects.json");
    std::fs::create_dir_all(home.join(".agentdeck")).map_err(|e| e.to_string())?;
    std::fs::write(file, serde_json::to_string_pretty(&known).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Removes only the selected project entry, preserving the remaining configuration.
pub(crate) fn remove_project_mcp(path: &Path, name: &str, is_toml: bool) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut root: Value = if is_toml { serde_json::to_value(toml::from_str::<toml::Value>(&text).map_err(|e| e.to_string())?).map_err(|e| e.to_string())? } else { serde_json::from_str(&text).map_err(|e| e.to_string())? };
    let key = if is_toml { "mcp_servers" } else { "mcpServers" };
    root[key].as_object_mut().ok_or("Invalid MCP server map")?.remove(name);
    let text = if is_toml { toml::to_string_pretty(&root).map_err(|e| e.to_string())? } else { serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? };
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_merge_preserves_entries_and_rejects_conflicts_or_invalid_files() {
        let dir = std::env::temp_dir().join(format!("agentdeck-sync-{}", super::super::stage::new_job_id()));
        std::fs::create_dir_all(&dir).expect("temp folder");
        for (file, key, toml_file) in [("mcp.json", "mcpServers", false), ("config.toml", "mcp_servers", true)] {
            let path = dir.join(file);
            merge_mcp(&path, key, "first", json!({"command":"one"}), toml_file).expect("first");
            merge_mcp(&path, key, "second", json!({"command":"two"}), toml_file).expect("second");
            let original = std::fs::read_to_string(&path).expect("read");
            assert!(original.contains("first") && original.contains("second"));
            assert!(merge_mcp(&path, key, "first", json!({"command":"changed"}), toml_file).is_err());
            assert_eq!(original, std::fs::read_to_string(&path).expect("unchanged"));
            remove_project_mcp(&path, "second", toml_file).expect("remove second");
            assert!(std::fs::read_to_string(&path).expect("read").contains("first"));
            std::fs::write(&path, "broken config {").expect("corrupt");
            assert!(merge_mcp(&path, key, "third", json!({}), toml_file).is_err());
            assert_eq!(std::fs::read_to_string(&path).expect("read"), "broken config {");
        }
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
