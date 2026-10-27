//! The addresses of providers, connectors and local model servers, from the one file the frontend reads too
//! (`src/lib/endpoints.json`, built in). Change an address there, not where it is used.

use std::sync::LazyLock;

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anthropic {
    pub console_bootstrap: String,
    /// With `{org}` for the organization's id.
    pub console_prepaid_credits: String,
}

#[derive(Deserialize)]
pub struct OpenRouter {
    pub key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGpt {
    pub home: String,
    pub usage: String,
    pub reset_credits: String,
    pub reset_credits_consume: String,
}

#[derive(Deserialize)]
pub struct Antigravity {
    pub quota: String,
}

#[derive(Deserialize)]
pub struct Web {
    pub search: String,
}

#[derive(Deserialize)]
pub struct Local {
    pub kind: String,
    pub label: String,
    pub base: String,
}

/// The versions of the third-party account tools that were read through and are installed (never `latest`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pins {
    pub codex_auth: String,
    pub claude_swap: String,
}

#[derive(Deserialize)]
pub struct Endpoints {
    pub pins: Pins,
    pub anthropic: Anthropic,
    pub openrouter: OpenRouter,
    pub chatgpt: ChatGpt,
    pub antigravity: Antigravity,
    pub web: Web,
    pub local: Vec<Local>,
}

const SOURCE: &str = include_str!("../../src/lib/endpoints.json");

/// The addresses, read once. The file is part of the build and checked by a test, so it always reads.
pub static ENDPOINTS: LazyLock<Endpoints> = LazyLock::new(|| {
    serde_json::from_str(SOURCE).unwrap_or_else(|error| panic!("src/lib/endpoints.json: {error}"))
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_endpoints_file_reads() {
        let endpoints: Endpoints = serde_json::from_str(SOURCE).expect("endpoints.json");
        assert!(endpoints.anthropic.console_prepaid_credits.contains("{org}"));
        assert_eq!(endpoints.local.len(), 3);
        assert!(endpoints.local.iter().all(|server| server.base.starts_with("http://127.0.0.1:")));
    }
}
