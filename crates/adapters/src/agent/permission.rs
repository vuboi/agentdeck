//! The app's permission modes, and what each one means for every agent.
//!
//! The Composer offers five modes. Each agent names its own (Claude's `--permission-mode`, Codex's sandbox and approval
//! policy, agy's `--mode`, OpenCode's `--auto`, Copilot's approval handler). An adapter reads the turn's mode through
//! [`Mode::parse`] and takes its own setting from the methods here, so a mode means the same thing for every agent and
//! a new agent maps onto these five without new names of its own.

/// A permission mode of the app, as the Composer's picker names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Every tool asks first.
    Ask,
    /// File edits run without asking; commands and everything else asks.
    Edits,
    /// Nothing is changed: reading runs, writing and commands are refused.
    Plan,
    /// Reading, writing and commands run without asking (commands in the sandbox); anything else asks.
    Auto,
    /// Nothing asks.
    Yolo,
}

impl Mode {
    /// The mode of a turn. Accepts the app's names and the CLI names of earlier builds (`acceptEdits`,
    /// `bypassPermissions`). A missing or unknown value is [`Mode::Ask`], the safe default.
    pub fn parse(value: Option<&str>) -> Mode {
        match value {
            Some("edits" | "acceptEdits") => Mode::Edits,
            Some("plan") => Mode::Plan,
            Some("auto") => Mode::Auto,
            Some("yolo" | "bypassPermissions") => Mode::Yolo,
            _ => Mode::Ask,
        }
    }

    /// Claude Code's `--permission-mode`. `None` keeps its default, which asks.
    pub fn claude(self) -> Option<&'static str> {
        match self {
            Mode::Ask => None,
            Mode::Edits => Some("acceptEdits"),
            Mode::Plan => Some("plan"),
            Mode::Auto => Some("auto"),
            Mode::Yolo => Some("bypassPermissions"),
        }
    }

    /// Codex's sandbox. `None` for [`Mode::Yolo`], which runs without a sandbox.
    pub fn codex_sandbox(self) -> Option<&'static str> {
        match self {
            Mode::Ask | Mode::Plan => Some("read-only"),
            Mode::Edits | Mode::Auto => Some("workspace-write"),
            Mode::Yolo => None,
        }
    }

    /// agy's `--mode`. `None` keeps its default; [`Mode::Yolo`] uses its own flag instead.
    pub fn agy(self) -> Option<&'static str> {
        match self {
            Mode::Edits => Some("accept-edits"),
            Mode::Plan => Some("plan"),
            Mode::Auto => Some("auto"),
            Mode::Ask | Mode::Yolo => None,
        }
    }

    /// Whether OpenCode may approve its own rule-matched calls without asking (`--auto`).
    pub fn opencode_auto(self) -> bool {
        matches!(self, Mode::Auto | Mode::Yolo)
    }
}

/// What a tool call does, as an adapter that decides by itself (an SDK handler, a custom runner) classifies it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Read,
    Write,
    Shell,
    Other,
}

/// What a mode does with one call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Runs without asking.
    Approve,
    /// Waits for the user.
    Ask,
    /// Refused, and the agent is told why.
    Refuse,
}

/// The verdict of a mode for one action. Adapters that decide themselves use this, so the five modes read the same
/// everywhere. A call the user already allowed is approved before this is asked.
pub fn verdict(mode: Mode, action: Action) -> Verdict {
    match (mode, action) {
        (Mode::Yolo, _) => Verdict::Approve,
        (Mode::Ask, _) => Verdict::Ask,
        (Mode::Edits, Action::Read | Action::Write) => Verdict::Approve,
        (Mode::Edits, _) => Verdict::Ask,
        // "Auto-approve: run commands in the sandbox": reads, edits and commands run; anything else asks.
        (Mode::Auto, Action::Read | Action::Write | Action::Shell) => Verdict::Approve,
        (Mode::Auto, _) => Verdict::Ask,
        (Mode::Plan, Action::Read) => Verdict::Approve,
        (Mode::Plan, _) => Verdict::Refuse,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_names_and_earlier_cli_names_read_the_same_mode() {
        assert_eq!(Mode::parse(Some("yolo")), Mode::Yolo);
        assert_eq!(Mode::parse(Some("bypassPermissions")), Mode::Yolo);
        assert_eq!(Mode::parse(Some("acceptEdits")), Mode::Edits);
        assert_eq!(Mode::parse(Some("edits")), Mode::Edits);
        assert_eq!(Mode::parse(Some("plan")), Mode::Plan);
        assert_eq!(Mode::parse(Some("auto")), Mode::Auto);
        assert_eq!(Mode::parse(Some("ask")), Mode::Ask);
        assert_eq!(Mode::parse(None), Mode::Ask);
        assert_eq!(Mode::parse(Some("something new")), Mode::Ask);
    }

    #[test]
    fn each_agent_gets_its_own_name_for_a_mode() {
        assert_eq!(Mode::Ask.claude(), None);
        assert_eq!(Mode::Yolo.claude(), Some("bypassPermissions"));
        assert_eq!(Mode::Ask.codex_sandbox(), Some("read-only"));
        assert_eq!(Mode::Yolo.codex_sandbox(), None);
        assert_eq!(Mode::Edits.agy(), Some("accept-edits"));
        assert_eq!(Mode::Yolo.agy(), None);
        assert!(Mode::Yolo.opencode_auto());
        assert!(!Mode::Edits.opencode_auto());
    }

    #[test]
    fn verdicts_follow_the_five_modes() {
        use Action::*;
        assert_eq!(verdict(Mode::Yolo, Shell), Verdict::Approve);
        assert_eq!(verdict(Mode::Ask, Read), Verdict::Ask);
        assert_eq!(verdict(Mode::Edits, Write), Verdict::Approve);
        assert_eq!(verdict(Mode::Edits, Shell), Verdict::Ask);
        assert_eq!(verdict(Mode::Auto, Write), Verdict::Approve);
        assert_eq!(verdict(Mode::Auto, Shell), Verdict::Approve);
        assert_eq!(verdict(Mode::Auto, Other), Verdict::Ask);
        assert_eq!(verdict(Mode::Plan, Read), Verdict::Approve);
        assert_eq!(verdict(Mode::Plan, Write), Verdict::Refuse);
        assert_eq!(verdict(Mode::Plan, Shell), Verdict::Refuse);
    }

}

