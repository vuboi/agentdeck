//! OpenCode, driven headless with `opencode run --format json`.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

use super::{AgentEvent, Provider, TurnRequest};

fn error_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.clone()),
        Value::Object(map) => {
            for key in ["message", "detail", "reason", "error", "data", "name"] {
                if let Some(text) = map.get(key).and_then(error_text) {
                    return Some(text);
                }
            }
            None
        }
        _ => None,
    }
}

pub struct OpenCode {
    session: Mutex<Option<String>>,
    tools: Mutex<HashSet<String>>,
}

impl Default for OpenCode {
    fn default() -> Self { Self { session: Mutex::new(None), tools: Mutex::new(HashSet::new()) } }
}

impl Provider for OpenCode {
    fn id(&self) -> &'static str { "opencode" }
    fn binary(&self) -> &'static str { "opencode" }
    fn candidates(&self) -> Vec<PathBuf> { Vec::new() }
    fn args(&self, req: &TurnRequest) -> Vec<String> {
        let mut args = vec!["run".into(), "--format".into(), "json".into()];
        if let Some(model) = &req.model { args.extend(["--model".into(), model.clone()]); }
        if let (Some(session), true) = (&req.session_id, req.resume) { args.extend(["--session".into(), session.clone()]); }
        if matches!(req.permission_mode.as_deref(), Some("auto") | Some("bypassPermissions")) { args.push("--auto".into()); }
        args.push(req.prompt.clone());
        args
    }
    fn environment(&self, req: &TurnRequest) -> Vec<(String, String)> {
        let Ok(bridge) = std::env::var("AGENTDECK_BRIDGE") else { return Vec::new() };
        let url = super::mcp_url(&bridge, req.owner.as_deref(), req.cwd.as_deref());
        vec![("OPENCODE_CONFIG_CONTENT".into(), serde_json::json!({
            "mcp": { "agentdeck": { "type": "remote", "url": url, "enabled": true } }
        }).to_string())]
    }
    fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
        let Ok(value) = serde_json::from_str::<Value>(line) else { return Vec::new() };
        let session = value["sessionID"].as_str().or_else(|| value["session_id"].as_str()).map(str::to_owned);
        if let Some(id) = &session { if let Ok(mut current) = self.session.lock() { *current = Some(id.clone()); } }
        let kind = value["type"].as_str().unwrap_or_default();
        let part = &value["part"];
        let text = value["text"].as_str().or_else(|| part["text"].as_str()).unwrap_or_default();
        match kind {
            "text" | "message" | "agent_message" => (!text.is_empty()).then_some(AgentEvent::Delta { text: text.to_owned() }).into_iter().collect(),
            "reasoning" | "thinking" => (!text.is_empty()).then_some(AgentEvent::Thinking { text: text.to_owned() }).into_iter().collect(),
            "step_finish" => {
                let tokens = &part["tokens"];
                vec![AgentEvent::Usage { input_tokens: tokens["input"].as_u64().unwrap_or(0), output_tokens: tokens["output"].as_u64().unwrap_or(0) + tokens["reasoning"].as_u64().unwrap_or(0), cache_read_tokens: tokens["cache"]["read"].as_u64().unwrap_or(0), cache_write_tokens: tokens["cache"]["write"].as_u64().unwrap_or(0), new_call: true }]
            }
            "tool_use" => {
                let id = part["callID"].as_str().or_else(|| part["id"].as_str()).unwrap_or("opencode-tool").to_owned();
                let name = part["tool"].as_str().unwrap_or("tool").to_owned();
                let input = part["state"]["input"].to_string();
                let status = part["state"]["status"].as_str().unwrap_or_default();
                if status == "completed" || status == "error" {
                    let output = part["state"]["output"].as_str().or_else(|| part["state"]["error"].as_str()).unwrap_or_default().to_owned();
                    vec![AgentEvent::ToolResult { id, output, is_error: status == "error", image: None }]
                } else if self.tools.lock().map(|mut calls| calls.insert(id.clone())).unwrap_or(false) {
                    vec![AgentEvent::ToolCall { id, name, input, change: None, background: false }]
                } else { Vec::new() }
            }
            "done" | "session_end" => vec![AgentEvent::Done { session_id: session, text: String::new(), cost_usd: value["cost"].as_f64() }],
            "error" => vec![AgentEvent::Error { message: error_text(&value["error"]).or_else(|| (!text.is_empty()).then_some(text.to_owned())).unwrap_or_else(|| "OpenCode failed".into()) }],
            _ => Vec::new(),
        }
    }
    fn finish(&self, _req: &TurnRequest) -> Option<AgentEvent> {
        self.session.lock().ok().and_then(|mut id| id.take()).map(|session_id| AgentEvent::Done { session_id: Some(session_id), text: String::new(), cost_usd: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_text_usage_tools_and_finish() {
        let provider = OpenCode::default();
        assert_eq!(provider.parse_line(r#"{"type":"text","sessionID":"ses_1","part":{"text":"hi"}}"#), vec![AgentEvent::Delta { text: "hi".into() }]);
        assert!(matches!(&provider.parse_line(r#"{"type":"tool_use","part":{"callID":"c1","tool":"bash","state":{"status":"running","input":{"command":"pwd"}}}}"#)[0], AgentEvent::ToolCall { name, .. } if name == "bash"));
        assert!(matches!(provider.finish(&TurnRequest { prompt: String::new(), session_id: None, resume: false, model: None, permission_mode: None, effort: None, allowed_tools: Vec::new(), cwd: None, owner: None }), Some(AgentEvent::Done { session_id: Some(id), .. }) if id == "ses_1"));
    }

    #[test]
    fn parses_error_shapes_from_opencode() {
        let provider = OpenCode::default();
        assert_eq!(provider.parse_line(r#"{"type":"error","error":{"type":"provider.auth","message":"Not logged in"}}"#), vec![AgentEvent::Error { message: "Not logged in".into() }]);
        assert_eq!(provider.parse_line(r#"{"type":"error","error":{"data":{"message":"Provider unavailable"}}}"#), vec![AgentEvent::Error { message: "Provider unavailable".into() }]);
    }
}
