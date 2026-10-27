//! Plugins of Claude Code and Codex. They change only through `claude plugin` and `codex plugin`; this module reads their
//! `--json` lists and runs their commands.

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::connectors::{capture, failure, which};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PluginItem {
    /// `name@marketplace`, the form the CLIs take.
    pub id: String,
    pub name: String,
    pub marketplace: String,
    pub version: String,
    pub agent: String,
    pub enabled: bool,
    /// Codex has no enable/disable command, so its plugins can only be removed.
    pub toggleable: bool,
}

/// A plugin id goes to a CLI as one argument: it may not be empty, start with `-` or hold spaces.
pub fn check_plugin_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.starts_with('-') || id.chars().any(char::is_whitespace) {
        Err(format!("not a valid plugin id: {id}"))
    } else {
        Ok(())
    }
}

/// Runs one agent CLI with its arguments (no shell) and returns stdout.
pub async fn run(agent: &str, args: &[&str]) -> Result<String, String> {
    let binary = match agent {
        "claude" | "codex" => agent,
        _ => return Err(format!("{agent} has no plugin command")),
    };
    let program = which(binary).ok_or_else(|| format!("{binary} is not installed"))?;
    let out = capture(&program, args, Duration::from_secs(120)).await?;
    if out.ok {
        Ok(out.stdout)
    } else {
        Err(failure(&out))
    }
}

pub fn parse_plugins(agent: &str, value: &Value) -> Vec<PluginItem> {
    // Claude prints an array; Codex prints `{ "installed": [...] }`.
    let rows = if agent == "codex" {
        value["installed"].as_array()
    } else {
        value.as_array()
    };
    rows.map(|rows| {
        rows.iter()
            .filter_map(|row| {
                if agent == "codex" && row["installed"].as_bool() == Some(false) {
                    return None;
                }
                let id = if agent == "codex" {
                    row["pluginId"].as_str()?
                } else {
                    row["id"].as_str()?
                }
                .to_owned();
                let (name, marketplace) = match id.split_once('@') {
                    Some((name, marketplace)) => (name.to_owned(), marketplace.to_owned()),
                    None => (id.clone(), String::new()),
                };
                Some(PluginItem {
                    name: row["name"].as_str().map(str::to_owned).unwrap_or(name),
                    marketplace: row["marketplaceName"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or(marketplace),
                    version: row["version"].as_str().unwrap_or_default().to_owned(),
                    agent: agent.to_owned(),
                    enabled: row["enabled"].as_bool().unwrap_or(false),
                    toggleable: agent == "claude",
                    id,
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

/// The plugins one agent has installed.
pub async fn installed(agent: &str) -> Result<Vec<PluginItem>, String> {
    let text = run(agent, &["plugin", "list", "--json"]).await?;
    let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
    Ok(parse_plugins(agent, &value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_plugin_rows_are_read() {
        let value = json!([{ "id": "caveman@caveman", "version": "3.1.0", "enabled": true }]);
        let plugins = parse_plugins("claude", &value);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "caveman");
        assert_eq!(plugins[0].marketplace, "caveman");
        assert!(plugins[0].enabled && plugins[0].toggleable);
    }

    #[test]
    fn codex_plugin_rows_skip_uninstalled_and_cannot_toggle() {
        let value = json!({ "installed": [
            { "pluginId": "pdf@openai", "name": "pdf", "marketplaceName": "openai", "version": "1", "enabled": true, "installed": true },
            { "pluginId": "old@openai", "name": "old", "marketplaceName": "openai", "version": "0", "enabled": false, "installed": false }
        ]});
        let plugins = parse_plugins("codex", &value);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, "pdf@openai");
        assert!(!plugins[0].toggleable);
    }

    #[test]
    fn ids_that_could_read_as_options_are_refused() {
        assert!(check_plugin_id("tool@market").is_ok());
        assert!(check_plugin_id("--yes").is_err());
        assert!(check_plugin_id("a b").is_err());
    }
}
