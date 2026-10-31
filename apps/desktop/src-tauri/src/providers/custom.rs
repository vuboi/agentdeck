//! Runtime helpers for generic OpenAI/Anthropic/Ollama-compatible providers.

use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
pub(crate) struct Probe {
    pub(crate) ok: bool,
    pub(crate) models: Vec<String>,
    pub(crate) ms: u64,
    pub(crate) error: Option<String>,
}

pub(crate) fn models_url(kind: &str, base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    match kind {
        "ollama" => format!("{base}/api/tags"),
        "anthropic" => format!("{}/v1/models", base.trim_end_matches("/v1")),
        _ => format!("{base}/models"),
    }
}

pub(crate) fn parse_models(kind: &str, json: &Value) -> Vec<String> {
    let (list, key) = if kind == "ollama" {
        (&json["models"], "name")
    } else {
        (&json["data"], "id")
    };
    list.as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item[key].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) async fn probe(
    kind: &str,
    base: &str,
    key: Option<&str>,
    key_env: Option<&str>,
    limit: Duration,
) -> Probe {
    let started = Instant::now();
    let done = |models: Vec<String>, error: Option<String>| Probe {
        ok: error.is_none(),
        models,
        ms: started.elapsed().as_millis() as u64,
        error,
    };
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return done(
            Vec::new(),
            Some("the address must start with http:// or https://".to_string()),
        );
    }
    let client = match reqwest::Client::builder().timeout(limit).build() {
        Ok(client) => client,
        Err(error) => return done(Vec::new(), Some(error.to_string())),
    };
    let mut request = client.get(models_url(kind, base));
    let key = match (
        key.filter(|key| !key.is_empty()),
        key_env.filter(|name| !name.is_empty()),
    ) {
        (Some(key), _) => Some(key.to_owned()),
        (None, Some(name)) => match std::env::var(name) {
            Ok(key) => Some(key),
            Err(_) => {
                return done(
                    Vec::new(),
                    Some(format!("environment variable {name} is not set")),
                )
            }
        },
        (None, None) => None,
    };
    if let Some(key) = key {
        request = if kind == "anthropic" {
            request
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01")
        } else {
            request.bearer_auth(key)
        };
    }
    match request.send().await {
        Ok(response) if response.status().is_success() => match response.json::<Value>().await {
            Ok(json) => done(parse_models(kind, &json), None),
            Err(error) => done(Vec::new(), Some(format!("not a model list: {error}"))),
        },
        Ok(response) => done(
            Vec::new(),
            Some(format!("the server answered {}", response.status())),
        ),
        Err(error) if error.is_timeout() => done(Vec::new(), Some("no answer in time".to_string())),
        Err(error) if error.is_connect() => done(
            Vec::new(),
            Some("could not connect: is the server running?".to_string()),
        ),
        Err(error) => done(Vec::new(), Some(error.to_string())),
    }
}
