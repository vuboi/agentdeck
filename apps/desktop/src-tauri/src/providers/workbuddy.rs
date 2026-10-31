//! WorkBuddy's OpenAI-shaped runtime rules.

use reqwest::RequestBuilder;
use serde_json::Value;

pub(crate) fn prepare_messages(messages: &[Value]) -> Vec<Value> {
    let mut result = messages.to_vec();
    let Some(last_user) = result
        .iter()
        .rposition(|message| message["role"].as_str() == Some("user"))
    else {
        return result;
    };
    let mut trimmed = Vec::with_capacity(result.len() - last_user + 1);
    if let Some(system) = result
        .first()
        .filter(|message| message["role"].as_str() == Some("system"))
    {
        trimmed.push(system.clone());
    }
    trimmed.extend(result.drain(last_user..));
    for message in &mut trimmed {
        sanitize_strings(message);
    }
    trimmed
}

pub(crate) async fn authorize(
    request: RequestBuilder,
    fallback_key: Option<&str>,
) -> Result<RequestBuilder, String> {
    let key = match crate::connectors::workbuddy::access_token().await {
        Ok(key) => key,
        Err(_) => fallback_key.unwrap_or_default().to_string(),
    };
    if key.is_empty() {
        return Err("WorkBuddy is not signed in".into());
    }
    Ok(request
        .bearer_auth(key)
        .header("X-Product", "SaaS")
        .header("X-Domain", "www.workbuddy.ai")
        .header("X-IDE-Type", "WorkBuddy")
        .header("X-IDE-Name", "WorkBuddy")
        .header("X-IDE-Version", "5.5.6")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("X-Agent-Intent", "craft")
        .header("X-Agent-Purpose", "conversation")
        .header("X-Agent-Type", "main")
        .header("User-Agent", "WorkBuddy/5.5.6"))
}

fn sanitize_strings(value: &mut Value) {
    match value {
        Value::String(text) => {
            for (from, to) in [
                ("x-anthropic-billing-header:", ""),
                (
                    "You are Claude Code, Anthropic's official CLI for Claude",
                    "You are Claude Code, Anthropic's CLI for Claude",
                ),
                (
                    "Codex CLI is an open source project led by OpenAI.",
                    "The Codex CLI is an open source project led by OpenAI.",
                ),
                ("Codex CLI", "the assistant"),
                ("codex_cli_rs", "assistant"),
                ("OpenAI", "the provider"),
                ("Codex", "the assistant"),
            ] {
                *text = text.replace(from, to);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(sanitize_strings),
        Value::Object(object) => object.values_mut().for_each(sanitize_strings),
        _ => {}
    }
}
