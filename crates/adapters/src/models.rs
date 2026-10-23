//! The models each agent CLI offers, read from what the CLI itself keeps on disk.
//!
//! Codex caches its server's catalogue in `~/.codex/models_cache.json` (the same list `codex debug models`
//! prints); the entries with `visibility: "list"` are the ones its picker shows, each with its reasoning levels.
//! Claude Code has no catalogue command: its picker is the current Opus / Sonnet / Haiku plus the extra models
//! the account is offered, which it caches in `~/.claude.json` as `additionalModelOptionsCache`.

use std::path::Path;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelInfo {
    /// What the CLI takes as `--model` / `-m`.
    pub value: String,
    pub label: String,
    pub description: String,
    /// Reasoning levels the model accepts, in order.
    pub efforts: Vec<String>,
    pub default_effort: Option<String>,
}

const CLAUDE_EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

fn claude_model(value: &str, label: &str, description: &str, default_effort: &str) -> ModelInfo {
    ModelInfo {
        value: value.into(),
        label: label.into(),
        description: description.into(),
        efforts: CLAUDE_EFFORTS.iter().map(|level| level.to_string()).collect(),
        default_effort: Some(default_effort.into()),
    }
}

/// Claude Code's models: the three current tiers, then what the account is additionally offered.
pub fn claude(home: &Path) -> Vec<ModelInfo> {
    let mut models = vec![
        claude_model("claude-opus-5-5", "Opus 5.5", "Most capable for complex work", "medium"),
        claude_model("claude-sonnet-5-5", "Sonnet 5.5", "Fast and capable for everyday tasks", "high"),
        claude_model("claude-haiku-5-5", "Haiku 5.5", "Fastest for quick answers", "medium"),
    ];
    let config = std::fs::read_to_string(home.join(".claude.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    for extra in config
        .as_ref()
        .and_then(|config| config["additionalModelOptionsCache"].as_array())
        .into_iter()
        .flatten()
    {
        let Some(value) = extra["value"].as_str() else { continue };
        if models.iter().any(|model| model.value == value) {
            continue;
        }
        let label = extra["label"].as_str().unwrap_or(value);
        // "Fable 5.1 · Most capable…": the version goes on the label, the rest is the description.
        let about = extra["description"].as_str().unwrap_or_default();
        let (version, description) = about.split_once(" · ").or_else(|| about.split_once(" – ")).unwrap_or(("", about));
        let label = if version.starts_with(label) { version } else { label };
        models.push(claude_model(value, label, description.trim(), "high"));
    }
    models
}

/// Codex's models from its cached catalogue, in its own order. Empty when Codex has not run yet.
pub fn codex(codex_home: &Path) -> Vec<ModelInfo> {
    let Some(catalogue) = std::fs::read_to_string(codex_home.join("models_cache.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return Vec::new();
    };
    let mut entries: Vec<&Value> = catalogue["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|model| model["visibility"] == "list")
        .collect();
    entries.sort_by_key(|model| model["priority"].as_i64().unwrap_or(i64::MAX));
    entries
        .into_iter()
        .filter_map(|model| {
            let value = model["slug"].as_str()?;
            Some(ModelInfo {
                value: value.into(),
                label: model["display_name"].as_str().unwrap_or(value).replace('-', " "),
                description: model["description"].as_str().unwrap_or_default().into(),
                efforts: model["supported_reasoning_levels"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|level| level["effort"].as_str().map(str::to_owned))
                    .collect(),
                default_effort: model["default_reasoning_level"].as_str().map(str::to_owned),
            })
        })
        .collect()
}

const LEVELS: [&str; 3] = ["low", "medium", "high"];

/// Antigravity's models from the output of `agy models`: one `<id>\t<label>` line each. The reasoning level is
/// part of the id (`gemini-3.8-flash-high`), so ids that differ only by it become one model with levels, which
/// `agy --model <base> --effort <level>` takes. A model with a single level keeps its full id.
pub fn antigravity(listing: &str) -> Vec<ModelInfo> {
    let entries: Vec<(&str, &str)> = listing
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(id, label)| (id.trim(), label.trim()))
        .filter(|(id, _)| !id.is_empty())
        .collect();
    // The id without its level, and the level, when the id ends in one.
    let split = |id: &str| LEVELS.iter().find_map(|level| id.strip_suffix(&format!("-{level}")).map(|base| (base.to_string(), *level)));
    let mut models: Vec<ModelInfo> = Vec::new();
    let mut folded: Vec<String> = Vec::new();
    for (id, label) in &entries {
        let Some((base, _)) = split(id) else {
            models.push(plain(id, label));
            continue;
        };
        if folded.contains(&base) {
            continue;
        }
        let levels: Vec<&str> = entries.iter().filter_map(|(other, _)| split(other).filter(|(b, _)| *b == base).map(|(_, level)| level)).collect();
        if levels.len() < 2 {
            models.push(plain(id, label));
            continue;
        }
        let efforts: Vec<String> = LEVELS.iter().filter(|level| levels.contains(level)).map(|level| level.to_string()).collect();
        let default_effort = efforts.iter().find(|level| *level == "medium").or(efforts.first()).cloned();
        let name = label.rsplit_once(" (").map_or(*label, |(name, _)| name);
        models.push(ModelInfo { value: base.clone(), label: name.to_string(), description: String::new(), efforts, default_effort });
        folded.push(base);
    }
    models
}

fn plain(id: &str, label: &str) -> ModelInfo {
    ModelInfo { value: id.into(), label: label.into(), description: String::new(), efforts: Vec::new(), default_effort: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("agentdeck-models-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn codex_lists_only_the_visible_models_with_their_levels() {
        let dir = scratch("codex");
        std::fs::write(
            dir.join("models_cache.json"),
            r#"{"models":[
                {"slug":"gpt-b","display_name":"GPT-B","visibility":"list","priority":2,"supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}],"default_reasoning_level":"low"},
                {"slug":"hidden","visibility":"hide","priority":0},
                {"slug":"gpt-a","display_name":"GPT-A","visibility":"list","priority":1}
            ]}"#,
        )
        .unwrap();
        let models = codex(&dir);
        assert_eq!(models.iter().map(|m| m.value.as_str()).collect::<Vec<_>>(), ["gpt-a", "gpt-b"]);
        assert_eq!(models[1].efforts, ["low", "high"]);
        assert_eq!(models[1].default_effort.as_deref(), Some("low"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn claude_adds_the_models_the_account_is_offered() {
        let dir = scratch("claude");
        std::fs::write(
            dir.join(".claude.json"),
            r#"{"additionalModelOptionsCache":[{"value":"claude-fable-5-1[1m]","label":"Fable","description":"Fable 5.1 · Most capable for your hardest tasks"}]}"#,
        )
        .unwrap();
        let models = claude(&dir);
        assert_eq!(models.len(), 4);
        assert_eq!((models[3].value.as_str(), models[3].label.as_str()), ("claude-fable-5-1[1m]", "Fable 5.1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn antigravity_folds_reasoning_levels_into_one_model() {
        let listing = "Fetching available models...
gemini-4-argon-medium	Gemini 4 Argon (Medium)
gemini-4-argon-high	Gemini 4 Argon (High)
gemini-4-argon-low	Gemini 4 Argon (Low)
claude-opus-5-5-low	Claude Opus 5.5 (Low)
claude-opus-5-5-high	Claude Opus 5.5 (High)
gpt-oss-120b-medium	GPT-OSS 120B (Medium)
";
        let models = antigravity(listing);
        assert_eq!(models.iter().map(|m| m.value.as_str()).collect::<Vec<_>>(), ["gemini-4-argon", "claude-opus-5-5", "gpt-oss-120b-medium"]);
        assert_eq!(models[0].label, "Gemini 4 Argon");
        assert_eq!(models[0].efforts, ["low", "medium", "high"]);
        assert_eq!(models[0].default_effort.as_deref(), Some("medium"));
        assert_eq!(models[1].efforts, ["low", "high"]);
        assert_eq!(models[1].default_effort.as_deref(), Some("low"));
        // A model with one level keeps its full id: there is nothing to choose.
        assert_eq!((models[2].label.as_str(), models[2].efforts.len()), ("GPT-OSS 120B (Medium)", 0));
    }

    #[test]
    fn antigravity_without_a_listing_has_no_models() {
        assert!(antigravity("").is_empty());
        assert!(antigravity("Fetching available models...
").is_empty());
    }
}
