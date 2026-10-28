//! Detects a turn that hangs silently: a CLI that is alive but prints nothing for too long (a lost network
//! connection it never reports, a deadlock). Any output resets it, so a long tool run that prints now and then is
//! fine. Kept apart from the loops that use it (`runner.rs` today; `warm.rs` can use the same type).
//!
//! ```ignore
//! let mut dog = Watchdog::new(provider.stall_limit());
//! loop {
//!     tokio::select! {
//!         line = next_line() => { dog.touch(); /* handle line */ }
//!         _ = tokio::time::sleep_until(dog.deadline()) => { /* dog.expired(): report dog.message(), end the tree */ }
//!     }
//! }
//! ```

use std::time::Duration;

use tokio::time::Instant;

use super::AgentEvent;

/// How long a turn may print nothing while one of its tools runs: the CLI writes nothing until a build or a test suite
/// returns, so the ordinary limit would end real work. Still bounded, so a tool that hangs is caught.
pub const TOOL_STALL_LIMIT: Duration = Duration::from_secs(2 * 60 * 60);

/// How long a turn may print nothing before it is called stuck. Long on purpose: a build, a test run or a model
/// that thinks for minutes writes nothing in between.
pub const DEFAULT_STALL_LIMIT: Duration = Duration::from_secs(10 * 60);

/// Time since the last sign of life. Uses tokio's clock, so tests can pause and advance it.
#[derive(Debug, Clone)]
pub struct Watchdog {
    limit: Duration,
    last: Instant,
    /// Tool calls started and not answered yet: while one runs, the limit is `TOOL_STALL_LIMIT`.
    open_tools: usize,
}

impl Watchdog {
    pub fn new(limit: Duration) -> Self {
        Self {
            limit,
            last: Instant::now(),
            open_tools: 0,
        }
    }

    /// Records output: the limit counts from now again.
    pub fn touch(&mut self) {
        self.last = Instant::now();
    }

    /// Follows the turn's tool calls, so the time a tool takes is not taken for a hang.
    pub fn note(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::ToolCall { .. } => self.open_tools += 1,
            AgentEvent::ToolResult { .. } => self.open_tools = self.open_tools.saturating_sub(1),
            _ => {}
        }
    }

    fn current_limit(&self) -> Duration {
        if self.open_tools > 0 {
            self.limit.max(TOOL_STALL_LIMIT)
        } else {
            self.limit
        }
    }

    /// The moment the limit runs out unless `touch` is called first (for `sleep_until` / `timeout_at`).
    pub fn deadline(&self) -> Instant {
        self.last + self.current_limit()
    }

    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline()
    }

    /// The error text for a stall.
    pub fn message(&self) -> String {
        format!(
            "agent produced no output for {} s",
            self.current_limit().as_secs().max(1)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_running_tool_is_not_taken_for_a_hang() {
        let mut dog = Watchdog::new(Duration::from_secs(60));
        dog.note(&AgentEvent::ToolCall { id: "t".into(), name: "Bash".into(), input: "cargo test".into(), change: None, background: false });
        tokio::time::advance(Duration::from_secs(30 * 60)).await;
        assert!(!dog.expired(), "a long build is not a stall");
        dog.note(&AgentEvent::ToolResult { id: "t".into(), output: String::new(), is_error: false, image: None, change: None });
        assert!(dog.expired(), "after the tool the ordinary limit applies again");
    }

    #[tokio::test(start_paused = true)]
    async fn expires_after_the_limit_and_touch_resets_it() {
        let mut dog = Watchdog::new(Duration::from_secs(60));
        assert!(!dog.expired());
        tokio::time::advance(Duration::from_secs(59)).await;
        assert!(!dog.expired());
        dog.touch();
        tokio::time::advance(Duration::from_secs(59)).await;
        assert!(!dog.expired(), "touch must restart the count");
        tokio::time::advance(Duration::from_secs(2)).await;
        assert!(dog.expired());
    }

    #[tokio::test(start_paused = true)]
    async fn deadline_wakes_a_sleeping_select() {
        let dog = Watchdog::new(Duration::from_secs(600));
        let start = Instant::now();
        tokio::time::sleep_until(dog.deadline()).await;
        assert!(dog.expired());
        assert_eq!(start.elapsed(), Duration::from_secs(600));
        assert_eq!(dog.message(), "agent produced no output for 600 s");
    }
}
