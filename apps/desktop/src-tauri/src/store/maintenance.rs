//! Source editing and explicit npm MCP update discovery. No runtime command is executed during checks.
use serde::Serialize;
use super::{library, lock};

/// Read or edit a managed skill's Git source without changing installed content.
#[tauri::command]
pub fn store_skill_source(name: String, source: Option<String>, subpath: Option<String>) -> Result<lock::LockEntry, String> {
    library::checked_name(&name)?;
    let home = crate::home_folder()?;
    if let Some(source) = source {
        let source = super::catalog::checked_source(&source)?;
        let subpath = subpath.unwrap_or_default().replace('\\', "/");
        if subpath.contains(':') || std::path::Path::new(&subpath).components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            return Err("Skill folder must be a relative path inside the repository".into());
        }
        return lock::set_source(&home, &name, source, subpath);
    }
    lock::read(&home).remove(&name).ok_or_else(|| "Skill has no managed source metadata".into())
}

#[derive(Serialize)]
pub struct PackageUpdate { id: String, current: String, latest: Option<String>, changed: bool, error: Option<String> }

fn version(text: &str) -> Option<(u64, u64, u64)> {
    let parts = text.split('.').collect::<Vec<_>>();
    if parts.len() != 3 { return None; }
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?))
}

/// Recognize only an explicit npm package version in known package runners.
fn package(command: &str, args: &[String]) -> Option<(usize, String, String)> {
    let command = command.rsplit(['/', '\\']).next()?.trim_end_matches(".cmd").trim_end_matches(".exe");
    let start = match command { "npx" | "bunx" => 0, "pnpm" if args.first().is_some_and(|s| s == "dlx") => 1, _ => return None };
    let (index, spec) = args.iter().enumerate().skip(start).find(|(_, a)| !a.starts_with('-'))?;
    let (name, current) = spec.rsplit_once('@')?;
    if name.is_empty() || name.len() > 214 || !name.chars().all(|c| c.is_ascii_alphanumeric() || "@/._-".contains(c)) || name.contains("..") { return None; }
    version(current)?;
    Some((index, name.into(), current.into()))
}

async fn latest(name: &str) -> Result<String, String> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().map_err(|e| e.to_string())?;
    let mut url = reqwest::Url::parse("https://registry.npmjs.org/").map_err(|e| e.to_string())?;
    url.path_segments_mut().map_err(|_| "Invalid registry URL")?.push(name).push("latest");
    let result: serde_json::Value = client.get(url).send().await.map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?.json().await.map_err(|e| e.to_string())?;
    let latest = result["version"].as_str().ok_or("Registry did not return a version")?;
    version(latest).ok_or("Registry version is not a stable semantic version")?;
    Ok(latest.into())
}

/// Compare configured npm pins with the registry. HTTP endpoints, local binaries and floating tags are not guessed.
#[tauri::command]
pub async fn store_mcp_updates() -> Vec<PackageUpdate> {
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut packages = std::collections::BTreeMap::new();
    for raw in crate::mcp_manage::read_all() {
        let id = crate::mcp_manage::raw_id(&raw);
        if !seen.insert(id.clone()) { continue; }
        let Some((_, name, current)) = package(raw.command.as_deref().unwrap_or_default(), &raw.args) else { continue; };
        if !packages.contains_key(&name) { packages.insert(name.clone(), latest(&name).await); }
        let (latest, error) = match &packages[&name] { Ok(v) => (Some(v.clone()), None), Err(e) => (None, Some(e.clone())) };
        let changed = latest.as_deref().is_some_and(|v| version(v) > version(&current));
        results.push(PackageUpdate { id, current, latest, changed, error });
    }
    results
}

/// Advance a pinned npm package after an explicit user update; preserve every other MCP setting.
#[tauri::command]
pub async fn store_mcp_update(id: String) -> Result<(), String> {
    let raw = crate::mcp_manage::read_all().into_iter().find(|r| crate::mcp_manage::raw_id(r) == id).ok_or("MCP source is no longer available")?;
    let command = raw.command.ok_or("This MCP does not have a versioned package source")?;
    let (index, name, current) = package(&command, &raw.args).ok_or("This MCP does not have an explicit supported npm version")?;
    let newest = latest(&name).await?;
    if version(&newest) <= version(&current) { return Ok(()); }
    let mut args = raw.args;
    args[index] = format!("{name}@{newest}");
    crate::mcp_providers::edit_source(&id, Some(command), Some(args))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_recognized_pinned_packages_are_checked() {
        assert_eq!(package("pnpm", &["dlx".into(), "@scope/tool@1.2.3".into()]), Some((1, "@scope/tool".into(), "1.2.3".into())));
        assert!(package("npx", &["-y".into(), "tool@latest".into()]).is_none());
        assert!(package("node", &["tool@1.2.3".into()]).is_none());
        assert!(package("pnpm", &["exec".into(), "tool@1.2.3".into()]).is_none());
        assert!(version("1.10.0") > version("1.9.9"));
    }
}
