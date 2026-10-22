//! Dev servers AgentDeck runs for the agent (`browser_serve`). They belong to the app, not to the agent's shell:
//! a server the CLI starts inside one of its commands dies when that command or the turn ends, and the Preview
//! is left on a refused connection. Output goes to `~/.agentdeck/servers/<port>.log`.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

static RUNNING: Mutex<Vec<(u16, Child)>> = Mutex::new(Vec::new());

/// The log file of the server on `port`.
pub fn log_path(port: u16) -> Result<PathBuf, String> {
    Ok(crate::home_folder()?.join(".agentdeck").join("servers").join(format!("{port}.log")))
}

/// Starts `command` in `cwd` for the server expected on `port`. A server this app started earlier on the same
/// port that has exited is forgotten first.
pub fn start(command: &str, cwd: &str, port: u16) -> Result<PathBuf, String> {
    let log = log_path(port)?;
    std::fs::create_dir_all(log.parent().ok_or("no folder for the server log")?).map_err(|error| error.to_string())?;
    let out = std::fs::File::create(&log).map_err(|error| error.to_string())?;
    let err = out.try_clone().map_err(|error| error.to_string())?;
    let mut shell = if cfg!(windows) {
        let mut shell = Command::new("cmd");
        shell.args(["/C", command]);
        shell
    } else {
        let mut shell = Command::new("sh");
        shell.args(["-c", command]);
        shell
    };
    shell.current_dir(cwd).stdin(Stdio::null()).stdout(out).stderr(err);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        shell.creation_flags(CREATE_NO_WINDOW);
    }
    let child = shell.spawn().map_err(|error| format!("could not start `{command}` in {cwd}: {error}"))?;
    let mut running = RUNNING.lock().map_err(|_| "busy".to_string())?;
    running.retain_mut(|(other, child)| *other != port || matches!(child.try_wait(), Ok(None)));
    running.push((port, child));
    Ok(log)
}

/// Whether the server this app started for `port` has already exited (a failed start).
pub fn exited(port: u16) -> bool {
    let Ok(mut running) = RUNNING.lock() else {
        return false;
    };
    running
        .iter_mut()
        .filter(|(other, _)| *other == port)
        .all(|(_, child)| !matches!(child.try_wait(), Ok(None)))
}

/// The last lines of a server's log, to explain a start that failed.
pub fn tail(port: u16, lines: usize) -> String {
    let Ok(path) = log_path(port) else {
        return String::new();
    };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}
