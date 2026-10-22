//! Agent CLIs behind one event stream.
//!
//! Each provider knows its own flags and its own output format. The rest of the app only sees
//! [`AgentEvent`], so adding a CLI means adding one [`Provider`] impl and nothing else.

pub mod claude;
pub mod runner;

use serde::Serialize;

/// One thing an agent did or said, in the shape the chat UI renders.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Visible answer text, as it streams.
    Delta { text: String },
    /// Reasoning text, as it streams.
    Thinking { text: String },
    /// A tool the agent ran; `input` is the JSON it passed.
    ToolCall { name: String, input: String },
    /// The turn finished. `session_id` is the CLI's id for the conversation, to resume next turn.
    Done {
        session_id: Option<String>,
        text: String,
        cost_usd: Option<f64>,
    },
    /// The turn failed.
    Error { message: String },
}

/// One user message to send to an agent.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnRequest {
    pub prompt: String,
    /// The CLI's conversation id. `None` starts a new conversation.
    pub session_id: Option<String>,
    /// `true` continues `session_id`; `false` creates a conversation with that id.
    pub resume: bool,
    pub model: Option<String>,
    /// Folder the CLI runs in. Claude files a conversation under the folder it ran from, so this is the project.
    pub cwd: Option<std::path::PathBuf>,
}

/// Everything provider-specific: how to start a turn and how to read its output.
pub trait Provider: Send + Sync {
    /// Short name, used in events and logs.
    fn id(&self) -> &'static str;
    /// Executable to run, looked up on PATH unless `candidates` says otherwise.
    fn binary(&self) -> &'static str;
    /// Extra places to look for the binary, tried before PATH (for apps started without a shell PATH).
    fn candidates(&self) -> Vec<std::path::PathBuf>;
    /// Command-line arguments for one turn.
    fn args(&self, req: &TurnRequest) -> Vec<String>;
    /// One line of stdout to zero or more events. Lines this provider does not understand give none.
    fn parse_line(&self, line: &str) -> Vec<AgentEvent>;
}
