//! The Store catalog (phase 3): what can be installed, from four places.
//!
//! - Plugins of Claude Code and Codex: the lists their own CLIs print with `--available`, with each plugin's source.
//! - MCP servers: the official registry (`registry.modelcontextprotocol.io`). Only npm and pypi packages are pinned to a
//!   version; remote http servers use their address.
//! - Skills: folders with a `SKILL.md` in git repositories the user added as sources.
//!
//! The list is cached in `~/.agentdeck/cache/catalog.json` and rebuilt only when the user refreshes it. Installing never
//! trusts the list: a plugin is scanned at its pinned source before the CLI installs it, a skill is scanned after it is
//! cloned, and an MCP server is shown with its exact command for the user to accept.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};


use super::stage;

const REGISTRY: &str = "https://registry.modelcontextprotocol.io/v0/servers";
/// The registry pages are read at most this many times, so a refresh has a fixed cost.
const REGISTRY_PAGES: usize = 20;
const REGISTRY_PAGE: usize = 100;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CatalogItem {
    /// Unique within the catalog.
    pub key: String,
    /// `plugin`, `mcp` or `skill`.
    pub kind: String,
    pub name: String,
    pub summary: String,
    pub version: String,
    /// The marketplace, the registry namespace or the repository the item comes from.
    pub publisher: String,
    /// The agents that can use it.
    pub agents: Vec<String>,
    pub install: Install,
}

/// How an item is installed. The UI sends the whole item back, so this is what the install step trusts.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "how", rename_all = "lowercase")]
pub enum Install {
    /// A plugin of one agent: its source is scanned, then the agent's CLI installs `plugin_id`.
    Plugin {
        agent: String,
        plugin_id: String,
        source: PluginSource,
    },
    /// An MCP server: the spec `mcp_add` takes (name, transport, target, args, env, headers, agents).
    Mcp { spec: Value },
    /// A skill folder inside a git repository (`subpath` is empty for the repository root).
    Skill { url: String, subpath: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PluginSource {
    /// A folder on this computer.
    Local { path: String },
    /// A folder of a git repository at a fixed commit.
    Git { url: String, sha: String, path: String },
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Cache {
    pub fetched_at: u64,
    pub items: Vec<CatalogItem>,
    /// What could not be read on the last refresh, one line each.
    #[serde(default)]
    pub errors: Vec<String>,
}

pub fn cache_file(home: &Path) -> PathBuf {
    home.join(".agentdeck").join("cache").join("catalog.json")
}

pub fn sources_file(home: &Path) -> PathBuf {
    home.join(".agentdeck").join("store-sources.json")
}

pub fn read_sources(home: &Path) -> Vec<String> {
    std::fs::read_to_string(sources_file(home))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn write_sources(home: &Path, urls: &[String]) -> Result<(), String> {
    let file = sources_file(home);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&file, serde_json::to_string_pretty(urls).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

/// A source the user can add: an https git address, nothing else.
pub fn checked_source(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/').to_owned();
    if !url.starts_with("https://") || url.contains(char::is_whitespace) || url.len() > 300 {
        return Err("a skill source must be an https:// git address".to_owned());
    }
    Ok(url)
}

pub fn read_cache(home: &Path) -> Option<Cache> {
    std::fs::read_to_string(cache_file(home))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn write_cache(home: &Path, cache: &Cache) -> Result<(), String> {
    let file = cache_file(home);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&file, serde_json::to_string(cache).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Rebuilds the whole catalog and writes the cache. A source that fails is listed in `errors`; the rest still shows.
pub async fn refresh(home: &Path) -> Result<Cache, String> {
    // The four kinds of source do not depend on each other, so they are read at the same time. Skill repositories are
    // cloned one after another, so a long list does not start many clones at once.
    let sources = read_sources(home);
    let skill_lists = async {
        let mut lists = Vec::new();
        for url in &sources {
            lists.push(skill_items(home, url).await);
        }
        lists
    };
    let (claude, codex, registry, skills) = tokio::join!(
        plugin_items(home, "claude"),
        plugin_items(home, "codex"),
        registry_items(),
        skill_lists,
    );
    let mut items = Vec::new();
    let mut errors = Vec::new();
    items.extend(claude);
    items.extend(codex);
    match registry {
        Ok(found) => items.extend(found),
        Err(error) => errors.push(format!("MCP registry: {error}")),
    }
    for (url, result) in sources.iter().zip(skills) {
        match result {
            Ok(found) => items.extend(found),
            Err(error) => errors.push(format!("{url}: {error}")),
        }
    }
    let cache = Cache { fetched_at: now(), items, errors };
    write_cache(home, &cache)?;
    Ok(cache)
}

/// The plugins one CLI offers, read from its `plugin list --available --json` output.
async fn plugin_items(home: &Path, agent: &str) -> Vec<CatalogItem> {
    let Ok(text) = super::plugins::run(agent, &["plugin", "list", "--available", "--json"]).await else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let rows = if agent == "codex" {
        value["available"].as_array()
    } else {
        value["available"].as_array().or_else(|| value.as_array())
    };
    let marketplaces = home.join(".claude").join("plugins").join("marketplaces");
    rows.map(|rows| {
        rows.iter()
            .filter(|row| row["installed"].as_bool() != Some(true))
            .filter_map(|row| plugin_item(agent, row, &marketplaces))
            .collect()
    })
    .unwrap_or_default()
}

/// One plugin row. A source that cannot be pinned or found on disk is skipped, so every item can be scanned.
fn plugin_item(agent: &str, row: &Value, marketplaces: &Path) -> Option<CatalogItem> {
    let plugin_id = row["pluginId"].as_str()?.to_owned();
    let name = row["name"].as_str()?.to_owned();
    let marketplace = row["marketplaceName"].as_str().unwrap_or_default().to_owned();
    let source = &row["source"];
    let source = match source {
        Value::String(relative) => PluginSource::Local {
            path: marketplaces.join(&marketplace).join(relative.trim_start_matches("./")).to_string_lossy().into_owned(),
        },
        Value::Object(_) => match source["source"].as_str()? {
            "local" => PluginSource::Local { path: source["path"].as_str()?.to_owned() },
            "git-subdir" | "url" => PluginSource::Git {
                url: source["url"].as_str()?.to_owned(),
                sha: source["sha"].as_str().unwrap_or_default().to_owned(),
                path: source["path"].as_str().unwrap_or_default().to_owned(),
            },
            _ => return None,
        },
        _ => return None,
    };
    if let PluginSource::Git { sha, .. } = &source {
        if sha.is_empty() {
            return None;
        }
    }
    Some(CatalogItem {
        key: format!("plugin:{agent}:{plugin_id}"),
        kind: "plugin".to_owned(),
        name,
        summary: row["description"].as_str().unwrap_or_default().to_owned(),
        version: row["version"].as_str().unwrap_or_default().to_owned(),
        publisher: marketplace,
        agents: vec![agent.to_owned()],
        install: Install::Plugin { agent: agent.to_owned(), plugin_id, source },
    })
}

/// Reads the official MCP registry page by page. Only npm and pypi packages and http remotes are kept.
async fn registry_items() -> Result<Vec<CatalogItem>, String> {
    let client = reqwest::Client::new();
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..REGISTRY_PAGES {
        let mut query = vec![("limit", REGISTRY_PAGE.to_string())];
        if let Some(next) = &cursor {
            query.push(("cursor", next.clone()));
        }
        let page: Value = client
            .get(REGISTRY)
            .query(&query)
            .timeout(Duration::from_secs(20))
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json()
            .await
            .map_err(|error| error.to_string())?;
        for entry in page["servers"].as_array().into_iter().flatten() {
            if let Some(item) = registry_item(&entry["server"]) {
                items.push(item);
            }
        }
        cursor = page["metadata"]["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    items.sort_by(|a, b| a.key.cmp(&b.key));
    items.dedup_by(|a, b| a.key == b.key);
    Ok(items)
}

/// Turns one registry server into an MCP add spec. The name is made safe for `mcp_add`.
fn registry_item(server: &Value) -> Option<CatalogItem> {
    let registry_name = server["name"].as_str()?;
    let name: String = registry_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
        .take(64)
        .collect();
    let agents = vec!["claude".to_owned(), "codex".to_owned()];
    let remote = server["remotes"]
        .as_array()
        .and_then(|list| list.iter().find(|remote| remote["type"] == "streamable-http"));
    let package = server["packages"].as_array().and_then(|list| {
        list.iter()
            .find(|package| matches!(package["registryType"].as_str(), Some("npm") | Some("pypi")))
    });
    let spec = if let Some(remote) = remote {
        json!({ "name": name, "transport": "http", "target": remote["url"], "args": [], "env": [], "headers": [], "agents": agents })
    } else if let Some(package) = package {
        let identifier = package["identifier"].as_str()?;
        let version = package["version"].as_str()?;
        let (command, args) = if package["registryType"] == "npm" {
            ("npx", vec!["-y".to_owned(), format!("{identifier}@{version}")])
        } else {
            ("uvx", vec![format!("{identifier}=={version}")])
        };
        json!({ "name": name, "transport": "stdio", "target": command, "args": args, "env": [], "headers": [], "agents": agents })
    } else {
        return None;
    };
    Some(CatalogItem {
        key: format!("mcp:{registry_name}"),
        kind: "mcp".to_owned(),
        name: server["title"].as_str().unwrap_or(registry_name).to_owned(),
        summary: server["description"].as_str().unwrap_or_default().to_owned(),
        version: server["version"].as_str().unwrap_or_default().to_owned(),
        publisher: registry_name.split('/').next().unwrap_or_default().to_owned(),
        agents,
        install: Install::Mcp { spec },
    })
}

/// The skills of one repository the user added. The clone is scanned for nothing here; it only lists the folders.
async fn skill_items(home: &Path, url: &str) -> Result<Vec<CatalogItem>, String> {
    let job = stage::new_job_id();
    let root = stage::stage(home, &job, stage::Source::Git { url: url.to_owned() }).await?;
    let found = stage::find_skills(&root);
    let items = found
        .into_iter()
        .map(|(name, path)| {
            let subpath = path.strip_prefix(&root).map(|rel| rel.to_string_lossy().replace('\\', "/")).unwrap_or_default();
            CatalogItem {
                key: format!("skill:{url}:{subpath}"),
                kind: "skill".to_owned(),
                summary: String::new(),
                version: String::new(),
                publisher: url.trim_start_matches("https://").to_owned(),
                agents: vec!["claude".to_owned(), "codex".to_owned()],
                name,
                install: Install::Skill { url: url.to_owned(), subpath },
            }
        })
        .collect();
    let _ = std::fs::remove_dir_all(stage::staging_root(home).join(&job));
    Ok(items)
}

/// The folder to scan for a plugin, with the commit it is pinned to. A git source is cloned at that commit.
pub async fn plugin_folder(home: &Path, job: &str, source: &PluginSource) -> Result<PathBuf, String> {
    match source {
        PluginSource::Local { path } => {
            let folder = PathBuf::from(path);
            if folder.is_dir() {
                Ok(folder)
            } else {
                Err("the plugin source folder is not on this computer".to_owned())
            }
        }
        PluginSource::Git { url, sha, path } => {
            let url = stage::checked_https(url)?;
            let dest = stage::staging_root(home).join(job);
            std::fs::create_dir_all(&dest).map_err(|error| error.to_string())?;
            stage::clone_at(url, sha, &dest).await?;
            Ok(dest.join(path.trim_start_matches("./")))
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claude_plugin_with_a_pinned_git_source_is_listed() {
        let row = json!({
            "pluginId": "tool@official", "name": "tool", "description": "Does things", "marketplaceName": "official",
            "source": { "source": "git-subdir", "url": "https://github.com/x/y.git", "path": "plugins/tool", "sha": "faf5305385de8afed9468904e8639be737aff39e" }
        });
        let item = plugin_item("claude", &row, Path::new("/m")).expect("listed");
        assert_eq!(item.kind, "plugin");
        assert_eq!(item.publisher, "official");
        assert!(matches!(item.install, Install::Plugin { ref plugin_id, .. } if plugin_id == "tool@official"));
    }

    #[test]
    fn a_plugin_without_a_pinned_commit_is_not_listed() {
        let row = json!({ "pluginId": "t@m", "name": "t", "marketplaceName": "m",
            "source": { "source": "git-subdir", "url": "https://github.com/x/y.git", "path": "p" } });
        assert!(plugin_item("claude", &row, Path::new("/m")).is_none());
    }

    #[test]
    fn a_registry_package_is_pinned_and_a_remote_is_an_address() {
        let npm = json!({ "name": "io.example/tools", "description": "d", "version": "1.0.0",
            "packages": [{ "registryType": "npm", "identifier": "@example/tools", "version": "2.1.0" }] });
        let item = registry_item(&npm).expect("listed");
        match item.install {
            Install::Mcp { spec } => {
                assert_eq!(spec["target"], "npx");
                assert_eq!(spec["args"][1], "@example/tools@2.1.0");
                assert_eq!(spec["name"], "io.example-tools");
            }
            _ => panic!("expected an MCP plan"),
        }
        let remote = json!({ "name": "io.example/remote", "remotes": [{ "type": "streamable-http", "url": "https://mcp.example.com/mcp" }] });
        assert!(registry_item(&remote).is_some());
    }

    #[test]
    fn only_https_sources_are_accepted() {
        assert!(checked_source("https://github.com/anthropics/skills/").is_ok());
        assert!(checked_source("http://example.com/x").is_err());
    }
}
