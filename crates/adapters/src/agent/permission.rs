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

/// A guard sees every shell command whatever the mode and may only refuse: it never approves anything, so no mode and no
/// earlier "always allow" can widen what it stops (the monotonic guards of DeepSeek Harness's tool pipeline,
/// https://github.com/deepseek-ai/deepseek-harness). It stops only what cannot be undone: erasing the root, the home folder or a
/// whole drive, writing to a disk device, formatting, a fork bomb. A command that merely looks risky is the mode's business.
/// The reason is told to the agent. `None`: the guard abstains.
pub fn guard_shell(command: &str) -> Option<&'static str> {
    let lowered = command.to_lowercase();
    let flat: String = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    // A fork bomb is written in one piece, with the pipes and `&` that segments are split on.
    if flat.replace(' ', "").contains(":(){:|:&};:") {
        return Some("blocked: this is a fork bomb.");
    }
    for segment in flat.split([';', '&', '|', '\n']).map(str::trim).filter(|part| !part.is_empty()) {
        let words: Vec<&str> = segment.split(' ').collect();
        // `sudo rm`, `command rm`, `env rm`: the command is the first word that is not one of these.
        let start = words.iter().position(|word| !matches!(*word, "sudo" | "command" | "env" | "doas" | "nohup" | "time")).unwrap_or(0);
        let words = &words[start..];
        let Some(first) = words.first().copied() else { continue };
        let rest = &words[1..];
        let unquote = |word: &str| word.trim_matches(['"', '\'']).to_string();
        match first {
            "rm" | "rmdir" | "del" | "erase" | "rd" => {
                let flags: String = rest.iter().filter(|word| word.starts_with('-') || word.starts_with('/')).flat_map(|word| word.chars().skip(1)).collect();
                let recursive = flags.contains('r') || flags.contains('s') || rest.contains(&"--recursive");
                // `/s` and `/q` of `del` and `rd` are switches, not the root.
                let is_switch = |word: &str| word.starts_with('-') || (first != "rm" && word.starts_with('/') && word.len() <= 3);
                if recursive && rest.iter().filter(|word| !is_switch(word)).any(|word| is_whole_place(&unquote(word))) {
                    return Some("blocked: this would erase the root, the home folder or a whole drive.");
                }
                if rest.contains(&"--no-preserve-root") {
                    return Some("blocked: --no-preserve-root removes the root folder.");
                }
            }
            "remove-item" | "ri" => {
                let recursive = rest.iter().any(|word| word.starts_with("-r"));
                let target = rest.iter().filter(|word| !word.starts_with('-')).map(|word| unquote(word)).find(|word| !word.is_empty());
                if recursive && target.as_deref().is_some_and(is_whole_place) {
                    return Some("blocked: this would erase a whole drive or the home folder.");
                }
            }
            "format" if rest.first().is_some_and(|word| word.len() == 2 && word.ends_with(':')) => return Some("blocked: this formats a drive."),
            word if word.starts_with("mkfs") => return Some("blocked: this formats a disk."),
            "dd" if rest.iter().any(|word| is_device(word.strip_prefix("of=").unwrap_or(""))) => return Some("blocked: this writes straight to a disk device."),
            _ => {}
        }
    }
    None
}

/// The root of a disk, the home folder, or what holds every user's folder.
fn is_whole_place(target: &str) -> bool {
    let target = target.trim_end_matches(['/', '\\']);
    matches!(target, "" | "~" | "$home" | "${home}" | "%userprofile%" | "$env:userprofile" | "/home" | "/users" | "/root" | "/*" | "~/*" | "$home/*")
        || (target.len() == 2 && target.ends_with(':') && target.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
        || (target.len() == 4 && target.ends_with(":\\*") && target.chars().next().is_some_and(|c| c.is_ascii_alphabetic()))
}

/// `/dev/sda`, `/dev/nvme0n1`, `/dev/disk2`: a whole disk or a partition, not `/dev/null` or a terminal.
fn is_device(path: &str) -> bool {
    ["/dev/sd", "/dev/hd", "/dev/vd", "/dev/xvd", "/dev/nvme", "/dev/disk", "/dev/mmcblk"].iter().any(|prefix| path.starts_with(prefix))
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

    #[test]
    fn the_guard_stops_what_cannot_be_undone_in_any_mode() {
        for command in [
            "rm -rf /",
            "rm -rf /*",
            "sudo rm -fr ~",
            "rm -r -f $HOME",
            "rm -rf \"$HOME/\"",
            "ls && rm -rf / --no-preserve-root",
            "Remove-Item -Recurse -Force C:\\",
            "Remove-Item C:\\ -Recurse",
            "rd /s /q C:\\",
            "format C:",
            "mkfs.ext4 /dev/sda1",
            "dd if=/dev/zero of=/dev/sda bs=1M",
            ":(){ :|:& };:",
        ] {
            assert!(guard_shell(command).is_some(), "{command}");
        }
    }

    #[test]
    fn the_guard_leaves_ordinary_commands_to_the_mode() {
        for command in [
            "rm -rf node_modules",
            "rm -rf ./target",
            "rm file.txt",
            "rm -rf /tmp/build",
            "git reset --hard",
            "Remove-Item -Recurse .\\dist",
            "dd if=/dev/zero of=./disk.img bs=1M count=10",
            "dd if=a of=/dev/null",
            "echo format C: is dangerous",
            "ls /home",
            "cargo clean",
        ] {
            assert_eq!(guard_shell(command), None, "{command}");
        }
    }
}
