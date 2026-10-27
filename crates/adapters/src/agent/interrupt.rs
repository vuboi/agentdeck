//! Stopping a running turn without ending its CLI.
//!
//! A stop used to drop the turn's future, which killed the whole process tree: the half-done step was lost and the next
//! message paid the CLI's start-up again. Now a stop is a message the CLI understands (an interrupt), the turn loop keeps
//! reading until the CLI reports the end of the turn, and the process goes back to the pool (`warm.rs`). Only a CLI that
//! does not answer within [`INTERRUPT_DEADLINE`], or that has no interrupt message, is killed (`runner::kill_tree`).
//!
//! This module holds what every turn loop shares: the token that carries the stop ([`StopHandle`] / [`StopToken`]), the
//! small state machine that turns it into "ask now" and "kill now" ([`Interrupter`]), the interrupt messages
//! themselves, and the event that closes a stopped turn.

use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::watch;
use tokio::time::Instant;

use super::AgentEvent;

/// How long a CLI gets to end the turn after it was asked to. After this the process tree is killed.
pub const INTERRUPT_DEADLINE: Duration = Duration::from_secs(30);

/// How a provider's CLI is told to stop the turn it is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptMode {
    /// A `control_request` line on stdin (Claude's `--input-format stream-json`).
    StreamJson,
    /// No interrupt message known: the process tree is killed at once (agy, OpenCode). Codex's app server has its own
    /// (`turn/interrupt`, see `codex_server.rs`).
    Kill,
}

/// The sending side of a stop: kept by whoever may stop the turn.
#[derive(Debug)]
pub struct StopHandle(watch::Sender<bool>);

/// The receiving side, handed to the turn. Cheap to clone.
#[derive(Debug, Clone)]
pub struct StopToken(watch::Receiver<bool>);

impl StopHandle {
    /// A handle and the token of the turn it stops.
    pub fn new() -> (StopHandle, StopToken) {
        let (sender, receiver) = watch::channel(false);
        (StopHandle(sender), StopToken(receiver))
    }

    /// Asks the turn to stop. Safe to call more than once.
    pub fn stop(&self) {
        self.0.send_replace(true);
    }
}

impl StopToken {
    /// A token nobody can fire, for a turn that is never stopped.
    pub fn never() -> StopToken {
        StopToken(watch::channel(false).1)
    }

    /// Whether a stop was asked for already.
    pub fn is_stopped(&self) -> bool {
        *self.0.borrow()
    }

    /// Resolves once a stop is asked for; never, if the handle is gone without one.
    pub async fn stopped(&mut self) {
        loop {
            if *self.0.borrow_and_update() {
                return;
            }
            if self.0.changed().await.is_err() {
                return std::future::pending().await;
            }
        }
    }
}

/// What the turn loop has to do next about a stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// The stop was just asked for: send the CLI its interrupt message.
    Ask,
    /// The CLI did not end the turn in time: kill it.
    Kill,
}

/// Turns a [`StopToken`] into the two moments a turn loop acts on. Used as one more branch of the loop's `select!`.
pub struct Interrupter {
    token: StopToken,
    deadline: Duration,
    kill_at: Option<Instant>,
}

impl Interrupter {
    pub fn new(token: StopToken) -> Interrupter {
        Interrupter { token, deadline: INTERRUPT_DEADLINE, kill_at: None }
    }

    /// Whether the CLI was asked to stop (the turn it reports next is a stopped one).
    pub fn asked(&self) -> bool {
        self.kill_at.is_some()
    }

    /// Resolves with [`Signal::Ask`] once, when the stop comes, then with [`Signal::Kill`] when the deadline passes.
    /// Cancel safe: dropping it between the two loses nothing.
    pub async fn signal(&mut self) -> Signal {
        match self.kill_at {
            Some(at) => {
                tokio::time::sleep_until(at).await;
                Signal::Kill
            }
            None => {
                self.token.stopped().await;
                self.kill_at = Some(Instant::now() + self.deadline);
                Signal::Ask
            }
        }
    }
}

/// The stdin line that interrupts Claude's current turn. The CLI answers with a `control_response`, then ends the turn
/// with a `result` and waits for the next message.
pub fn claude_interrupt_line(request_id: &str) -> String {
    json!({ "type": "control_request", "request_id": request_id, "request": { "subtype": "interrupt" } }).to_string()
}

/// Codex app server's JSON-RPC request that interrupts a turn.
pub fn codex_interrupt_request(id: u64, thread: &str, turn: &str) -> Value {
    json!({ "id": id, "method": "turn/interrupt", "params": { "threadId": thread, "turnId": turn } })
}

/// The event that closes a stopped turn. What the CLI reports for an interrupted turn is an error result (Claude) or a
/// failed turn (Codex): after a stop that is not a failure, and the conversation goes on.
pub fn stopped(event: AgentEvent, conversation: Option<&str>) -> AgentEvent {
    match event {
        AgentEvent::Error { .. } => AgentEvent::Done { session_id: conversation.map(str::to_owned), text: String::new(), cost_usd: None },
        other => other,
    }
}

/// The closing event for a turn that had to be killed.
pub fn killed(conversation: Option<&str>) -> AgentEvent {
    stopped(AgentEvent::Error { message: String::new() }, conversation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_claude_interrupt_is_a_control_request_line() {
        let line = claude_interrupt_line("stop-1");
        assert!(!line.contains('\n'));
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["type"], "control_request");
        assert_eq!(value["request_id"], "stop-1");
        assert_eq!(value["request"]["subtype"], "interrupt");
    }

    #[test]
    fn the_codex_interrupt_names_thread_and_turn() {
        let value = codex_interrupt_request(7, "th", "tu");
        assert_eq!(value["id"], 7);
        assert_eq!(value["method"], "turn/interrupt");
        assert_eq!(value["params"]["threadId"], "th");
        assert_eq!(value["params"]["turnId"], "tu");
    }

    #[test]
    fn a_stopped_turn_ends_as_done_not_as_a_failure() {
        let event = stopped(AgentEvent::Error { message: "x".into() }, Some("s1"));
        assert_eq!(event, AgentEvent::Done { session_id: Some("s1".into()), text: String::new(), cost_usd: None });
        let delta = AgentEvent::Delta { text: "a".into() };
        assert_eq!(stopped(delta.clone(), Some("s1")), delta);
    }

    #[tokio::test(start_paused = true)]
    async fn asks_when_stopped_then_kills_after_the_deadline() {
        let (handle, token) = StopHandle::new();
        let mut interrupter = Interrupter::new(token);
        assert!(!interrupter.asked());
        handle.stop();
        let start = Instant::now();
        assert_eq!(interrupter.signal().await, Signal::Ask);
        assert!(interrupter.asked());
        assert_eq!(interrupter.signal().await, Signal::Kill);
        assert!(start.elapsed() >= INTERRUPT_DEADLINE);
    }

    #[tokio::test(start_paused = true)]
    async fn a_token_that_is_never_fired_never_resolves() {
        let mut token = StopToken::never();
        assert!(!token.is_stopped());
        let waited = tokio::time::timeout(Duration::from_secs(3600), token.stopped()).await;
        assert!(waited.is_err());
    }
}
