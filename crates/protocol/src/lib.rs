//! Shared data types for AgentDeck: sessions, turns, events, quota and model profiles.
//!
//! Every type derives `specta::Type` so `packages/protocol-ts` can be generated from here.
//! Timestamps are unix milliseconds. They are `f64` in TypeScript so they never become `bigint`.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    Codex,
    Gemini,
    Antigravity,
}

/// A working folder. Sessions are grouped under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
}

/// One conversation. `parent_id` + `fork_seq` mark a fork: the history of the parent up to that
/// event is shared, everything after belongs to this session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Session {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub agent: AgentKind,
    pub model: String,
    #[specta(type = specta_typescript::Number)]
    pub created_at: i64,
    /// Git branch the session works on, if any. Not a conversation branch: that is `parent_id`.
    pub git_branch: Option<String>,
    pub parent_id: Option<String>,
    #[specta(type = Option<specta_typescript::Number>)]
    pub fork_seq: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    User,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Turn {
    pub id: String,
    pub session_id: String,
    pub role: TurnRole,
    #[specta(type = specta_typescript::Number)]
    pub started_at: i64,
    #[specta(type = Option<specta_typescript::Number>)]
    pub ended_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Pending,
    Running,
    Ok,
    Fail,
}

/// One append-only entry in the event store.
///
/// `seq` is the global insertion order. `hash` covers `prev_hash`, the session, the timestamp
/// and the serialized `kind`, so editing any stored event breaks the chain from that point on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Event {
    #[specta(type = specta_typescript::Number)]
    pub seq: i64,
    pub id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    #[specta(type = specta_typescript::Number)]
    pub ts: i64,
    pub kind: EventKind,
    pub prev_hash: String,
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    MessageDelta {
        text: String,
    },
    Thinking {
        text: String,
    },
    ToolCall {
        call_id: String,
        name: String,
        #[specta(type = specta_typescript::Any)]
        input: serde_json::Value,
        status: ToolStatus,
        output: Option<String>,
    },
    PermissionRequest {
        request_id: String,
        summary: String,
    },
    Diff {
        path: String,
        patch: String,
    },
    TerminalOutput {
        chunk: String,
    },
    Usage {
        #[specta(type = specta_typescript::Number)]
        tokens_in: u64,
        #[specta(type = specta_typescript::Number)]
        tokens_out: u64,
        #[specta(type = specta_typescript::Number)]
        cache_read: u64,
        #[specta(type = specta_typescript::Number)]
        cache_write: u64,
        cost_usd: Option<f64>,
    },
    AgentHandoff {
        from: AgentKind,
        to: AgentKind,
        artifact_id: Option<String>,
    },
    Artifact {
        id: String,
        kind: String,
        schema: String,
        #[specta(type = specta_typescript::Any)]
        content: serde_json::Value,
    },
    ModelSwitch {
        from: String,
        to: String,
        reason: String,
        #[specta(type = specta_typescript::Any)]
        quota_snapshot: serde_json::Value,
    },
    StatusChange {
        state: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum QuotaKind {
    H5,
    Weekly,
    Daily,
    Rpm,
    Tpm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum QuotaUnit {
    Pct,
    Tokens,
    Requests,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum QuotaSource {
    Exact,
    Estimated,
    Reactive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct QuotaWindow {
    pub provider: String,
    pub account: String,
    pub kind: QuotaKind,
    #[specta(type = specta_typescript::Number)]
    pub used: f64,
    #[specta(type = specta_typescript::Number)]
    pub limit: f64,
    pub unit: QuotaUnit,
    #[specta(type = Option<specta_typescript::Number>)]
    pub resets_at: Option<i64>,
    pub source: QuotaSource,
    #[specta(type = specta_typescript::Number)]
    pub observed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ModelProfile {
    pub id: String,
    pub provider: String,
    pub tier: String,
    #[specta(type = specta_typescript::Number)]
    pub context_window: u64,
    #[specta(type = specta_typescript::Number)]
    pub max_output: u64,
    #[specta(type = specta_typescript::Number)]
    pub price_in: f64,
    #[specta(type = specta_typescript::Number)]
    pub price_out: f64,
    #[specta(type = specta_typescript::Number)]
    pub price_cache_read: f64,
    #[specta(type = specta_typescript::Number)]
    pub price_cache_write: f64,
    pub tps_measured: Option<f64>,
}

/// Generates TypeScript declarations for every protocol type.
pub fn export_typescript() -> Result<String, specta_typescript::Error> {
    let types = specta::Types::default()
        .register::<AgentKind>()
        .register::<Project>()
        .register::<Session>()
        .register::<TurnRole>()
        .register::<Turn>()
        .register::<ToolStatus>()
        .register::<Event>()
        .register::<EventKind>()
        .register::<QuotaKind>()
        .register::<QuotaUnit>()
        .register::<QuotaSource>()
        .register::<QuotaWindow>()
        .register::<ModelProfile>();
    specta_typescript::Typescript::default().export(&types, specta_serde::Format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_kind_round_trips_with_type_tag() {
        let kind = EventKind::ToolCall {
            call_id: "c1".into(),
            name: "read_file".into(),
            input: serde_json::json!({ "path": "a.rs" }),
            status: ToolStatus::Running,
            output: None,
        };
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.starts_with(r#"{"type":"tool_call""#));
        let back: EventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind);
    }

    #[test]
    fn forked_session_round_trips() {
        let session = Session {
            id: "s2".into(),
            project_id: "p1".into(),
            title: "Try codex".into(),
            agent: AgentKind::Codex,
            model: "gpt-6-luna".into(),
            created_at: 1,
            git_branch: Some("feat/fork".into()),
            parent_id: Some("s1".into()),
            fork_seq: Some(42),
        };
        let json = serde_json::to_string(&session).unwrap();
        assert_eq!(serde_json::from_str::<Session>(&json).unwrap(), session);
    }

    #[test]
    fn agent_kind_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&AgentKind::Claude).unwrap(),
            r#""claude""#
        );
    }

    #[test]
    fn typescript_export_uses_plain_numbers_and_tags() {
        let ts = export_typescript().expect("export");
        assert!(
            ts.contains("seq: number,"),
            "required ids must not be nullable"
        );
        assert!(ts.contains("tokens_in: number;"));
        assert!(ts.contains(r#"type: "tool_call""#));
        assert!(ts.contains(r#""claude" | "codex""#));
    }

    #[test]
    fn committed_protocol_ts_matches_rust_types() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../packages/protocol-ts/src/index.ts"
        );
        let on_disk = std::fs::read_to_string(path).expect("read index.ts");
        assert_eq!(
            on_disk,
            export_typescript().expect("export"),
            "run `pnpm gen:types` to refresh packages/protocol-ts"
        );
    }
}
