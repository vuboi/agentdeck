//! A trace of how a turn moves, for finding out later why a chat hung: lock waits, which process took the turn, how long
//! the first line took, when a turn ended and why, when a tail started and stopped. One short note per step of the
//! flow, never per token. The app registers where the notes go (`set_sink`); without it they are dropped.
//!
//! Nothing here holds a lock or waits: a note is handed to the sink, which must not block either.

use std::sync::{Arc, OnceLock};

/// Where notes go: the chat, what kind of step, and a few words of detail.
pub type TraceSink = Arc<dyn Fn(&str, &str, &str) + Send + Sync>;

static SINK: OnceLock<TraceSink> = OnceLock::new();

/// Registers where the notes go. Called once by the app.
pub fn set_sink(sink: TraceSink) {
    let _ = SINK.set(sink);
}

/// What the heartbeat adds about the adapters: kept processes and tails.
pub fn stats() -> String {
    format!("pool={} tails={}", super::warm::pool_len(), super::tail::count())
}

/// Notes one step of a chat's flow.
pub fn note(chat: &str, kind: &str, detail: impl AsRef<str>) {
    if let Some(sink) = SINK.get() {
        sink(chat, kind, detail.as_ref());
    }
}
