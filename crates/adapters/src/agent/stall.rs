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

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use tokio::time::Instant;

use super::{AgentEvent, TaskStatus};

/// How long a turn may print nothing while one of its tools runs: the CLI writes nothing until a build or a test suite
/// returns, so the ordinary limit would end real work. Still bounded, so a tool that hangs is caught.
pub const TOOL_STALL_LIMIT: Duration = Duration::from_secs(2 * 60 * 60);

/// How long a turn may print nothing before it is called stuck. Long on purpose: a build, a test run or a model
/// that thinks for minutes writes nothing in between.
pub const DEFAULT_STALL_LIMIT: Duration = Duration::from_secs(10 * 60);

/// How long a turn that waits for a sub-agent may print nothing. A sub-agent reports on its way (`task_progress`, its
/// own steps, token counts), and each line counts as output; a child that is silent this long is stuck, and waiting the
/// two hours of an ordinary tool would leave the chat looking frozen.
pub const AGENT_STALL_LIMIT: Duration = Duration::from_secs(20 * 60);

/// Tool calls that start a sub-agent: their result does not say the work is over, and their own output is the child's.
fn starts_sub_agent(name: &str) -> bool {
    matches!(name, "Agent" | "Task" | "spawnAgent" | "subagent")
}

/// Time since the last sign of life. Uses tokio's clock, so tests can pause and advance it.
#[derive(Debug, Clone)]
pub struct Watchdog {
    limit: Duration,
    last: Instant,
    /// Tool calls started and not answered yet, by id, and whether each one runs a sub-agent. While an ordinary tool
    /// runs the limit is `TOOL_STALL_LIMIT`; while only sub-agents run it is `AGENT_STALL_LIMIT`.
    open_tools: HashMap<String, bool>,
    /// Sub-agents and background jobs the CLI reported (`Task` events), by id.
    open_tasks: HashSet<String>,
    /// The CLI said background work is open (a task list that is not empty), whatever the events said.
    background: bool,
}

impl Watchdog {
    pub fn new(limit: Duration) -> Self {
        Self {
            limit,
            last: Instant::now(),
            open_tools: HashMap::new(),
            open_tasks: HashSet::new(),
            background: false,
        }
    }

    /// Records whether the CLI has background work open.
    pub fn set_background(&mut self, open: bool) {
        self.background = open;
    }

    /// Records output: the limit counts from now again.
    pub fn touch(&mut self) {
        self.last = Instant::now();
    }

    /// Follows the turn's tool calls and tasks, so the time they take is not taken for a hang.
    pub fn note(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::ToolCall { id, name, .. } => {
                self.open_tools.insert(id.clone(), starts_sub_agent(name));
            }
            AgentEvent::ToolResult { id, .. } => {
                self.open_tools.remove(id);
            }
            AgentEvent::Task { id, status, .. } => match status {
                TaskStatus::Started | TaskStatus::Progress => {
                    self.open_tasks.insert(id.clone());
                }
                TaskStatus::Done | TaskStatus::Failed => {
                    self.open_tasks.remove(id);
                }
            },
            _ => {}
        }
    }

    fn current_limit(&self) -> Duration {
        if self.open_tools.values().any(|agent| !agent) {
            self.limit.max(TOOL_STALL_LIMIT)
        } else if !self.open_tools.is_empty() || !self.open_tasks.is_empty() || self.background {
            self.limit.max(AGENT_STALL_LIMIT)
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
    async fn a_sub_agent_that_says_nothing_is_stuck_long_before_an_ordinary_tool_would_be() {
        let mut dog = Watchdog::new(Duration::from_secs(10 * 60));
        dog.note(&AgentEvent::ToolCall { id: "a".into(), name: "Agent".into(), input: "scan".into(), change: None, background: true });
        tokio::time::advance(Duration::from_secs(19 * 60)).await;
        assert!(!dog.expired(), "a sub-agent may think for a while");
        tokio::time::advance(Duration::from_secs(2 * 60)).await;
        assert!(dog.expired(), "but not for the two hours of a build");
        // Its own output starts the count again.
        dog.touch();
        assert!(!dog.expired());
    }

    #[tokio::test(start_paused = true)]
    async fn an_ordinary_tool_beside_a_sub_agent_keeps_the_long_limit() {
        let mut dog = Watchdog::new(Duration::from_secs(10 * 60));
        dog.note(&AgentEvent::ToolCall { id: "a".into(), name: "Agent".into(), input: String::new(), change: None, background: true });
        dog.note(&AgentEvent::ToolCall { id: "b".into(), name: "Bash".into(), input: "cargo build".into(), change: None, background: false });
        tokio::time::advance(Duration::from_secs(60 * 60)).await;
        assert!(!dog.expired());
    }

    #[tokio::test(start_paused = true)]
    async fn open_tasks_and_background_work_allow_a_longer_silence_than_none() {
        let mut dog = Watchdog::new(Duration::from_secs(10 * 60));
        dog.note(&AgentEvent::Task { id: "t".into(), tool_use_id: None, status: TaskStatus::Started, label: None, summary: None, tools: None });
        tokio::time::advance(Duration::from_secs(15 * 60)).await;
        assert!(!dog.expired());
        dog.note(&AgentEvent::Task { id: "t".into(), tool_use_id: None, status: TaskStatus::Done, label: None, summary: None, tools: None });
        assert!(dog.expired(), "once the task ended the ordinary limit applies");
        dog.touch();
        dog.set_background(true);
        tokio::time::advance(Duration::from_secs(15 * 60)).await;
        assert!(!dog.expired());
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
