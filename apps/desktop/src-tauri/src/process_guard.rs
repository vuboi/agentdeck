//! Ending what the app started when the app itself ends (End task, a crash, a hang killed from the task manager), and
//! ending a whole process tree. One place per operating system, so that a new place that starts a long-lived process
//! calls `adopt_pid` and gets the same behaviour everywhere:
//!
//! - Windows: the process joins a job object that kills its members when the app's handle closes (servers.rs `job`).
//! - macOS and Linux: there is no job object (`PR_SET_PDEATHSIG` exists on Linux only, and only for the direct child), so
//!   a small `sh` watchdog is started beside each process. It checks every two seconds that the app still lives, and
//!   when the app has gone it ends the process and its group. It leaves with the process when that ends first.
//!
//! The list of every other place that differs by system is in `docs/platform-support.md`.

/// Ties a process the app started (by its id) to the app: it ends when the app ends, even when the app is killed.
pub fn adopt_pid(pid: u32) {
    // Listed first: if the app dies between here and the OS-level guard, the next start still ends it (process_registry).
    crate::process_registry::record(pid);
    #[cfg(windows)]
    crate::servers::adopt_pid_windows(pid);
    #[cfg(unix)]
    watch(pid);
}

/// Ends a process and everything it started. Fire and forget: the kill runs on its own.
pub fn kill_tree(pid: u32) {
    crate::process_registry::forget(pid);
    kill_tree_now(pid);
}

/// The kill itself, without the list (used at start-up, where the list was already emptied). The same function the agent
/// runner uses, so there is one way to end a tree.
pub fn kill_tree_now(pid: u32) {
    ag_adapters::agent::runner::kill_tree(pid);
}

/// The watchdog's script: while the app lives and the process lives, wait; when the app is gone, end the process.
#[cfg(any(unix, test))]
pub fn watchdog_script(app: u32, pid: u32) -> String {
    format!(
        "while kill -0 {app} 2>/dev/null; do kill -0 {pid} 2>/dev/null || exit 0; sleep 2; done; {}",
        ag_adapters::agent::runner::kill_script(pid)
    )
}

#[cfg(unix)]
fn watch(pid: u32) {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    // Its own group, so that ending the app's group does not end the watchdog before it has done its work.
    let _ = Command::new("sh")
        .args(["-c", &watchdog_script(std::process::id(), pid)])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watchdog_waits_for_the_app_and_leaves_when_the_process_ends_first() {
        let script = watchdog_script(100, 200);
        assert!(script.starts_with("while kill -0 100 2>/dev/null; do kill -0 200 2>/dev/null || exit 0; sleep 2; done;"));
        assert!(script.contains("kill -KILL -- -200"));
    }

    #[cfg(unix)]
    #[test]
    fn a_process_ends_when_its_app_is_gone() {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        // A stand-in "app" that dies at once, and a process to guard.
        let mut app = Command::new("sh").args(["-c", "sleep 30"]).stdin(Stdio::null()).spawn().unwrap();
        let mut guarded = Command::new("sh").args(["-c", "sleep 30"]).stdin(Stdio::null()).spawn().unwrap();
        let script = watchdog_script(app.id(), guarded.id());
        let mut dog = Command::new("sh").args(["-c", &script]).spawn().unwrap();
        app.kill().unwrap();
        let _ = app.wait();
        let started = Instant::now();
        while guarded.try_wait().unwrap().is_none() {
            assert!(started.elapsed() < Duration::from_secs(10), "the guarded process was not ended");
            std::thread::sleep(Duration::from_millis(200));
        }
        let _ = dog.wait();
    }
}
