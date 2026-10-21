//! Claude Code, driven headless: `claude -p <prompt> --output-format stream-json`.
//!
//! One process per turn. Context across turns comes from the conversation id: the first turn passes
//! `--session-id <uuid>`, later turns pass `--resume <uuid>`.

use std::path::PathBuf;

use serde_json::Value;

use super::{AgentEvent, Provider, TurnRequest};

pub struct Claude;

impl Provider for Claude {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn binary(&self) -> &'static str {
        "claude"
    }

    fn candidates(&self) -> Vec<PathBuf> {
        // The installer puts it in ~/.local/bin, which a desktop app started outside a shell may not see.
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        home.map(|home| vec![PathBuf::from(home).join(".local").join("bin").join("claude")]).unwrap_or_default()
    }

    fn args(&self, req: &TurnRequest) -> Vec<String> {
        let mut args = vec![
            "-p".to_string(),
            req.prompt.clone(),
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            // Token-level deltas, so text appears while it is written.
            "--include-partial-messages".to_string(),
        ];
        if let Some(id) = &req.session_id {
            args.push(if req.resume { "--resume" } else { "--session-id" }.to_string());
            args.push(id.clone());
        }
        if let Some(model) = &req.model {
            args.push("--model".to_string());
            args.push(model.clone());
        }
        args
    }

    fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match value["type"].as_str() {
            // Partial message chunks (needs --include-partial-messages).
            Some("stream_event") => {
                let delta = &value["event"]["delta"];
                match (value["event"]["type"].as_str(), delta["type"].as_str()) {
                    (Some("content_block_delta"), Some("text_delta")) => {
                        vec![AgentEvent::Delta { text: delta["text"].as_str().unwrap_or_default().to_string() }]
                    }
                    (Some("content_block_delta"), Some("thinking_delta")) => {
                        vec![AgentEvent::Thinking { text: delta["thinking"].as_str().unwrap_or_default().to_string() }]
                    }
                    _ => Vec::new(),
                }
            }
            // The full assistant message. Text is already streamed above, so only tool calls are taken here.
            Some("assistant") => value["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|block| block["type"] == "tool_use")
                .map(|block| AgentEvent::ToolCall {
                    name: block["name"].as_str().unwrap_or_default().to_string(),
                    input: block["input"].to_string(),
                })
                .collect(),
            Some("result") => {
                if value["is_error"].as_bool().unwrap_or(false) {
                    vec![AgentEvent::Error { message: value["result"].as_str().unwrap_or("agent failed").to_string() }]
                } else {
                    vec![AgentEvent::Done {
                        session_id: value["session_id"].as_str().map(str::to_string),
                        text: value["result"].as_str().unwrap_or_default().to_string(),
                        cost_usd: value["total_cost_usd"].as_f64(),
                    }]
                }
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(resume: bool) -> TurnRequest {
        TurnRequest { prompt: "hi".into(), session_id: Some("abc".into()), resume, model: None, cwd: None }
    }

    #[test]
    fn first_turn_creates_the_session_and_later_turns_resume_it() {
        assert!(Claude.args(&req(false)).windows(2).any(|w| w == ["--session-id", "abc"]));
        assert!(Claude.args(&req(true)).windows(2).any(|w| w == ["--resume", "abc"]));
    }

    #[test]
    fn text_deltas_stream_as_delta_events() {
        let line = r#"{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"po"}}}"#;
        assert_eq!(Claude.parse_line(line), vec![AgentEvent::Delta { text: "po".into() }]);
    }

    #[test]
    fn result_line_finishes_the_turn_with_the_session_id() {
        let line = r#"{"type":"result","subtype":"success","is_error":false,"result":"pong","session_id":"s1","total_cost_usd":0.5}"#;
        assert_eq!(
            Claude.parse_line(line),
            vec![AgentEvent::Done { session_id: Some("s1".into()), text: "pong".into(), cost_usd: Some(0.5) }]
        );
    }

    #[test]
    fn error_result_becomes_an_error_event() {
        let line = r#"{"type":"result","is_error":true,"result":"rate limited"}"#;
        assert_eq!(Claude.parse_line(line), vec![AgentEvent::Error { message: "rate limited".into() }]);
    }

    #[test]
    fn unknown_lines_are_ignored() {
        assert!(Claude.parse_line(r#"{"type":"system","subtype":"init"}"#).is_empty());
        assert!(Claude.parse_line("not json").is_empty());
    }
}
