//! Readers for the session files Claude Code and Codex already write to disk, plus `manage`
//! (rename, delete to trash). Only `manage` ever writes to those folders. Formats are not documented by the vendors, so the
//! parsers are tolerant: unknown line types are skipped, never fatal. See PLAN.md section 3b.

mod claude;
mod codex;
mod manage;
mod util;

pub use manage::{delete, rename};

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Claude,
    Codex,
}

/// One session found on disk, enough to draw a row in the sidebar tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistorySession {
    /// `claude:<uuid>` or `codex:<uuid>`, unique across sources.
    pub id: String,
    pub source: Source,
    pub title: String,
    /// The model recorded by the source CLI when it is available.
    pub model: Option<String>,
    /// Working folder of the session; sessions group into projects by it.
    pub cwd: String,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub message_count: usize,
    pub path: String,
    /// Set when this session starts with the same messages as an earlier one (a fork or a resume).
    pub parent_id: Option<String>,
    /// How many leading messages the two sessions share (capped, see `claude::SHARED_PREFIX`).
    pub fork_seq: Option<usize>,
}

/// One block of a message, in the shape the chat UI renders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryBlock {
    Thinking {
        text: String,
    },
    Response {
        text: String,
    },
    /// A tool call and its result. `status` is `ok` or `fail`.
    Command {
        command: String,
        output: String,
        status: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryMessage {
    pub id: String,
    /// `user` or `assistant`.
    pub role: String,
    /// ISO timestamp as written by the tool, empty when absent.
    pub time: String,
    pub blocks: Vec<HistoryBlock>,
    /// Tokens the answer used, summed over its model calls (Claude transcripts record them).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<HistoryUsage>,
    /// The model that wrote the answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct HistoryUsage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl HistoryUsage {
    fn add(&mut self, other: HistoryUsage) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }

    pub(crate) fn merge(slot: &mut Option<HistoryUsage>, other: Option<HistoryUsage>) {
        if let Some(other) = other {
            slot.get_or_insert_with(HistoryUsage::default).add(other);
        }
    }
}

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("path is outside the Claude and Codex history folders")]
    OutsideRoots,
    #[error("the session is open in a running tool; close it first")]
    InUse,
    #[error("{0}")]
    Invalid(String),
}

/// Where each tool keeps its sessions.
#[derive(Debug, Clone)]
pub struct Roots {
    pub claude: PathBuf,
    pub codex: PathBuf,
    /// `~/.codex/session_index.jsonl`: Codex's list of chat names.
    pub codex_index: PathBuf,
    /// Deleted sessions are moved here, never destroyed.
    pub trash: PathBuf,
}

impl Roots {
    /// `~/.claude/projects` and `~/.codex/sessions`.
    pub fn from_home(home: &Path) -> Self {
        Self {
            claude: home.join(".claude").join("projects"),
            codex: home.join(".codex").join("sessions"),
            codex_index: home.join(".codex").join("session_index.jsonl"),
            trash: home.join(".agentdeck").join("trash"),
        }
    }
}

/// Every session of both tools, newest first. Missing folders just yield nothing.
///
/// Ids are unique: resuming a Codex thread writes a second rollout file whose `session_meta` keeps the
/// original id, and two sessions with one id break keyed lists in the UI. The newest file wins.
pub fn list_all(roots: &Roots) -> Vec<HistorySession> {
    let mut sessions = claude::list(&roots.claude);
    sessions.extend(codex::list(&roots.codex, &roots.codex_index));
    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut seen = std::collections::HashSet::new();
    sessions.retain(|session| seen.insert(session.id.clone()));
    sessions
}

/// The transcript of one session. The path must live inside the history folders.
pub fn load(
    roots: &Roots,
    source: Source,
    path: &Path,
) -> Result<Vec<HistoryMessage>, HistoryError> {
    let root = match source {
        Source::Claude => &roots.claude,
        Source::Codex => &roots.codex,
    };
    let path = path.canonicalize()?;
    if !path.starts_with(root.canonicalize()?) {
        return Err(HistoryError::OutsideRoots);
    }
    Ok(match source {
        Source::Claude => claude::load(&path)?,
        Source::Codex => codex::load(&path)?,
    })
}

#[cfg(test)]
mod tests;
