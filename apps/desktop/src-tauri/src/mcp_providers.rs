//! Provider assignment for MCP. Disabled definitions live in the OS credential store;
//! the disk index contains only their identity and source path.
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::mcp_manage::{self, Raw};

#[derive(Serialize, Deserialize, Clone)]
struct Disabled { agent: String, scope: String, name: String, source: String, vault: String }

static MUTATIONS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn index() -> Result<PathBuf, String> { Ok(crate::home_folder()?.join(crate::channel::home_folder_name()).join("mcp-provider-disabled.json")) }
fn read_index() -> Result<Vec<Disabled>, String> {
    match std::fs::read_to_string(index()?) {
        Ok(text) => serde_json::from_str(&text).map_err(|_| "Disabled MCP index is invalid".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.to_string()),
    }
}
fn save_index(rows: &[Disabled]) -> Result<(), String> {
    let file = index()?;
    std::fs::create_dir_all(file.parent().ok_or("Missing parent")?).map_err(|e| e.to_string())?;
    let temporary = file.with_extension(format!("{}.tmp", crate::store::stage::new_job_id()));
    let result = (|| {
        std::fs::write(&temporary, serde_json::to_string_pretty(rows).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &file).map_err(|e| e.to_string())
    })();
    if result.is_err() { let _ = std::fs::remove_file(&temporary); }
    result
}

/// Removes disabled assignments from the index when the user uninstalls a server.
pub(crate) fn forget_disabled(name: &str, scope: &str, agents: &[String]) -> Result<Vec<String>, String> {
    let _guard = MUTATIONS.lock().map_err(|e| e.to_string())?;
    let mut rows = read_index()?;
    let removed = rows.iter().filter(|r| r.name == name && r.scope == scope && agents.contains(&r.agent))
        .map(|r| r.agent.clone()).collect::<Vec<_>>();
    let removed_rows = rows.iter().filter(|r| r.name == name && r.scope == scope && agents.contains(&r.agent)).cloned().collect::<Vec<_>>();
    rows.retain(|r| !(r.name == name && r.scope == scope && agents.contains(&r.agent)));
    if !removed.is_empty() { save_index(&rows)?; }
    #[cfg(windows)]
    for row in &removed_rows { crate::connectors::delete_secret(&row.vault)?; }
    #[cfg(not(windows))]
    let _ = removed_rows;
    Ok(removed)
}

#[cfg(windows)]
fn secret(key: &str) -> Option<String> { crate::connectors::stored_secret(key) }
#[cfg(not(windows))]
fn secret(_: &str) -> Option<String> { None }
#[cfg(windows)]
fn save_secret(key: &str, text: &str) -> Result<(), String> { crate::connectors::write_secret(key, text) }
#[cfg(not(windows))]
fn save_secret(_: &str, _: &str) -> Result<(), String> { Err("Secure MCP disable is currently supported on Windows".into()) }

/// Keep fully disabled servers discoverable without returning their secret values to the UI.
pub(crate) fn append_disabled(found: &mut Vec<Raw>) {
    for row in read_index().unwrap_or_default() {
        if found.iter().any(|r| r.agent == row.agent && r.scope == row.scope && r.name == row.name) { continue; }
        let Some(agent) = ["claude", "codex", "copilot", "antigravity"].into_iter().find(|a| *a == row.agent) else { continue; };
        let Some(value) = secret(&row.vault).and_then(|s| serde_json::from_str::<Value>(&s).ok()) else { continue; };
        let mut map = serde_json::Map::new();
        map.insert(row.name, value);
        for mut raw in mcp_manage::servers_in(&Value::Object(map), agent, &row.scope, &row.source) {
            raw.disabled = true;
            found.push(raw);
        }
    }
}

fn config(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    if path.extension().is_some_and(|e| e == "toml") {
        serde_json::to_value(toml::from_str::<toml::Value>(&text).map_err(|_| "MCP config is invalid; nothing changed")?).map_err(|e| e.to_string())
    } else { serde_json::from_str(&text).map_err(|_| "MCP config is invalid; nothing changed".into()) }
}

fn map<'a>(root: &'a mut Value, raw: &Raw) -> Result<&'a mut serde_json::Map<String, Value>, String> {
    let is_claude_user = Path::new(&raw.source).file_name().is_some_and(|n| n == ".claude.json");
    let root = if is_claude_user {
        if let Some(project) = raw.scope.strip_prefix("project: ") { &mut root["projects"][project] } else { root }
    } else { root };
    let key = if raw.agent == "codex" { "mcp_servers" } else { "mcpServers" };
    root[key].as_object_mut().ok_or_else(|| "Invalid MCP config map".into())
}

fn write_config(path: &Path, root: &Value) -> Result<(), String> {
    let text = if path.extension().is_some_and(|e| e == "toml") { toml::to_string_pretty(root).map_err(|e| e.to_string())? }
        else { serde_json::to_string_pretty(root).map_err(|e| e.to_string())? };
    std::fs::write(path, text).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct SourceDetails { pub target: String, pub args: Vec<String> }

/// Edit connection source while retaining provider-specific options, secrets and enabled state.
#[tauri::command]
pub async fn mcp_source(id: String, target: Option<String>, args: Option<Vec<String>>) -> Result<SourceDetails, String> {
    tauri::async_runtime::spawn_blocking(move || edit_source(&id, target, args)).await.map_err(|e| e.to_string())?
}

pub(crate) fn edit_source(id: &str, target: Option<String>, args: Option<Vec<String>>) -> Result<SourceDetails, String> {
    let _guard = MUTATIONS.lock().map_err(|e| e.to_string())?;
    let definitions = mcp_manage::read_all().into_iter().filter(|r| mcp_manage::raw_id(r) == id && !crate::channel::is_bridge_name(&r.name)).collect::<Vec<_>>();
    let first = definitions.first().ok_or("MCP source is no longer available")?;
    let Some(target) = target else { return Ok(SourceDetails { target: first.url.clone().or_else(|| first.command.clone()).unwrap_or_default(), args: first.args.clone() }); };
    let target = target.trim().to_owned();
    if target.is_empty() { return Err("Source cannot be empty".into()); }
    if first.url.is_some() && !target.starts_with("https://") && !target.starts_with("http://") { return Err("HTTP source must start with http:// or https://".into()); }
    let args = args.unwrap_or_else(|| first.args.clone());
    let index = read_index()?;
    let mut saved_files = std::collections::HashSet::new();
    for raw in &definitions {
        if !saved_files.insert((raw.source.clone(), raw.agent, raw.scope.clone())) { continue; }
        let saved = index.iter().find(|s| s.agent == raw.agent && s.scope == raw.scope && s.name == raw.name);
        let mut root = config(Path::new(&raw.source))?;
        let entries = map(&mut root, raw)?;
        let in_file = entries.contains_key(&raw.name);
        let mut value = if in_file { entries[&raw.name].clone() } else {
            let row = saved.ok_or("MCP definition is missing")?;
            serde_json::from_str::<Value>(&secret(&row.vault).ok_or("Saved MCP definition is unavailable")?).map_err(|_| "Saved MCP definition is invalid")?
        };
        patch_connection(&mut value, raw.url.is_some(), &target, &args)?;
        if in_file {
            entries.insert(raw.name.clone(), value);
            write_config(Path::new(&raw.source), &root)?;
        } else {
            save_secret(&saved.ok_or("Missing disabled definition")?.vault, &serde_json::to_string(&value).map_err(|e| e.to_string())?)?;
        }
    }
    Ok(SourceDetails { target, args })
}

/// Patch only connection location; environment, headers, timeouts and enabled flags remain provider-owned.
fn patch_connection(value: &mut Value, http: bool, target: &str, args: &[String]) -> Result<(), String> {
    let object = value.as_object_mut().ok_or("Invalid MCP definition")?;
    if http {
        let key = if object.contains_key("serverUrl") { "serverUrl" } else { "url" };
        object.insert(key.into(), json!(target));
    } else {
        object.insert("command".into(), json!(target));
        object.insert("args".into(), json!(args));
    }
    Ok(())
}

/// Enable/disable one provider in the row's own scope; native config is changed for future sessions.
#[tauri::command]
pub async fn mcp_provider_set(id: String, agent: String, enabled: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || set(&id, &agent, enabled)).await.map_err(|e| e.to_string())?
}

fn set(id: &str, agent: &str, enabled: bool) -> Result<(), String> {
    let _guard = MUTATIONS.lock().map_err(|e| e.to_string())?;
    if !["claude", "codex", "copilot", "antigravity"].contains(&agent) { return Err("Unsupported MCP provider".into()); }
    let raws = mcp_manage::read_all();
    let source = raws.iter().find(|r| mcp_manage::raw_id(r) == id && !crate::channel::is_bridge_name(&r.name)).ok_or("MCP server is no longer available")?;
    let definitions: std::collections::HashSet<_> = raws.iter().filter(|r| mcp_manage::raw_id(r) == id && r.agent == agent).map(|r| &r.source).collect();
    if definitions.len() > 1 { return Err("Multiple config files define this server for the provider; resolve duplicate definitions first".into()); }
    let current = raws.iter().find(|r| mcp_manage::raw_id(r) == id && r.agent == agent);
    let mut index = read_index()?;
    if let Some(raw) = current {
        let saved = index.iter().position(|r| r.agent == agent && r.scope == raw.scope && r.name == raw.name);
        let path = Path::new(&raw.source);
        let mut root = config(path)?;
        let entries = map(&mut root, raw)?;
        if enabled {
            if let Some(at) = saved {
                if entries.contains_key(&raw.name) { return Err("Existing config preserved; resolve it before restoring".into()); }
                let mut value: Value = serde_json::from_str(&secret(&index[at].vault).ok_or("Saved configuration is unavailable in Credential Manager")?).map_err(|e| e.to_string())?;
                value.as_object_mut().ok_or("Invalid server")?.remove("disabled");
                value.as_object_mut().ok_or("Invalid server")?.remove("enabled");
                entries.insert(raw.name.clone(), value);
                write_config(path, &root)?;
                let removed = index.remove(at);
                save_index(&index)?;
                #[cfg(windows)]
                crate::connectors::delete_secret(&removed.vault)?;
                #[cfg(not(windows))]
                let _ = removed;
                Ok(())
            } else {
                let value = entries.get_mut(&raw.name).ok_or("Server is no longer configured")?;
                value.as_object_mut().ok_or("Invalid server")?.remove("disabled");
                value.as_object_mut().ok_or("Invalid server")?.remove("enabled");
                if agent == "codex" { value["enabled"] = json!(true); }
                write_config(path, &root)
            }
        } else {
            if let Some(at) = saved {
                if !entries.contains_key(&raw.name) { return Ok(()); }
                index.remove(at);
            }
            let value = entries.remove(&raw.name).ok_or("Server is no longer configured")?;
            use sha2::{Digest, Sha256};
            let vault = format!("AgentDeck/MCP/{}", Sha256::digest(format!("{}|{}|{}", raw.source, agent, raw.name)).iter().map(|b| format!("{b:02x}")).collect::<String>());
            save_secret(&vault, &serde_json::to_string(&value).map_err(|e| e.to_string())?)?;
            index.push(Disabled { agent: agent.into(), scope: raw.scope.clone(), name: raw.name.clone(), source: raw.source.clone(), vault });
            save_index(&index)?;
            if let Err(error) = write_config(path, &root) {
                index.pop();
                save_index(&index)?;
                return Err(error);
            }
            Ok(())
        }
    } else if enabled {
        let home = crate::home_folder()?;
        let project = source.scope.strip_prefix("project: ");
        if project.is_some() && agent == "antigravity" { return Err("Antigravity project MCP config is not supported".into()); }
        let path = match (project, agent) {
            (Some(p), "claude") => Path::new(p).join(".mcp.json"),
            (Some(p), "codex") => Path::new(p).join(".codex/config.toml"),
            (Some(p), _) => Path::new(p).join(".copilot/mcp-config.json"),
            (None, "claude") => home.join(".claude.json"),
            (None, "codex") => crate::connectors::codex_home().join("config.toml"),
            (None, "antigravity") => home.join(".gemini/config/mcp_config.json"),
            (None, _) => home.join(".copilot/mcp-config.json"),
        };
        let mut value = if let Some(url) = &source.url { json!({"url":url}) } else { json!({"command":source.command,"args":source.args,"env":source.env}) };
        if !source.headers.is_empty() { value[if agent == "codex" {"http_headers"} else {"headers"}] = json!(source.headers); }
        if agent != "codex" { value["type"] = json!(source.transport); }
        crate::store::sync::merge_mcp(&path, if agent == "codex" {"mcp_servers"} else {"mcpServers"}, &source.name, value, agent == "codex")
    } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_sources_preserves_secrets_and_native_options() {
        let mut stdio = json!({"command":"old", "args":["old"], "env":{"TOKEN":"private"}, "enabled":false, "startup_timeout_sec":60});
        patch_connection(&mut stdio, false, "pnpm", &["dlx".into(), "tool@2.0.0".into()]).expect("edit");
        assert_eq!(stdio["env"]["TOKEN"], "private");
        assert_eq!(stdio["enabled"], false);
        assert_eq!(stdio["startup_timeout_sec"], 60);
        assert_eq!(stdio["command"], "pnpm");
        let mut http = json!({"serverUrl":"https://old", "headers":{"Authorization":"private"}, "disabled":true});
        patch_connection(&mut http, true, "https://new", &[]).expect("edit");
        assert_eq!(http["serverUrl"], "https://new");
        assert_eq!(http["headers"]["Authorization"], "private");
        assert_eq!(http["disabled"], true);
        assert!(http.get("url").is_none());
    }

    #[test]
    fn disable_and_restore_preserve_full_native_definition_and_other_entries() {
        for (source, agent, scope, mut root) in [
            (".claude.json", "claude", "project: C:/workspace", json!({"projects":{"C:/workspace":{"mcpServers":{"a":{"command":"run","customField":true},"b":{"url":"https://example.org"}}}},"theme":"dark"})),
            ("config.toml", "codex", "user", json!({"mcp_servers":{"a":{"command":"run","startup_timeout_sec":60},"b":{"command":"other"}},"model":"keep"})),
        ] {
            let raw = Raw { source: source.into(), agent, scope: scope.into(), ..Raw::default() };
            let original = root.clone();
            let saved = map(&mut root, &raw).expect("map").remove("a").expect("definition");
            assert!(map(&mut root, &raw).expect("map").contains_key("b"));
            assert!(!map(&mut root, &raw).expect("map").contains_key("a"));
            map(&mut root, &raw).expect("map").insert("a".into(), saved);
            assert_eq!(root, original);
        }
        let raw = Raw { source:".claude.json".into(), agent:"claude", scope:"user".into(), ..Raw::default() };
        assert!(map(&mut json!({"mcpServers":[]}), &raw).is_err());
    }
}
