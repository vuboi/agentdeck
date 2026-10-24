//! Dev servers AgentDeck runs for the agent (`browser_serve`). They belong to the app, not to the agent's shell:
//! a server the CLI starts inside one of its commands dies when that command or the turn ends, and the Preview
//! is left on a refused connection. Output goes to `~/.agentdeck/servers/<port>.log`.
//!
//! They also end with the app: on Windows every server joins a job object that kills its processes when the app
//! goes away (even if the app crashes); on exit every one is stopped with its children as well.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// The raw job handle (0: it could not be made, and servers then only stop through `kill_all`).
    static JOB: OnceLock<isize> = OnceLock::new();

    fn handle() -> Option<HANDLE> {
        let raw = *JOB.get_or_init(|| {
            // SAFETY: plain Win32 calls with a handle made here and a correctly sized, initialised struct.
            unsafe {
                let Ok(job) = CreateJobObjectW(None, None) else { return 0 };
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    std::ptr::addr_of!(info).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if set.is_err() {
                    return 0;
                }
                job.0 as isize
            }
        });
        (raw != 0).then_some(HANDLE(raw as *mut _))
    }

    /// Puts a started server (and, from now on, everything it starts) under the app's job.
    pub fn adopt(child: &std::process::Child) {
        if let Some(job) = handle() {
            // SAFETY: both handles are valid: the job lives for the whole run, the child is still owned by the caller.
            let _ = unsafe { AssignProcessToJobObject(job, HANDLE(child.as_raw_handle())) };
        }
    }
}

/// A server this app started: what it runs, where, and the address it answers at once known.
struct Server {
    key: u16,
    child: Child,
    command: String,
    cwd: String,
    url: Option<String>,
    started: std::time::SystemTime,
}

static RUNNING: Mutex<Vec<Server>> = Mutex::new(Vec::new());

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
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        shell.process_group(0); // its own group, so the whole tree can be stopped at once
    }
    let child = shell.spawn().map_err(|error| format!("could not start `{command}` in {cwd}: {error}"))?;
    #[cfg(windows)]
    job::adopt(&child);
    let mut running = RUNNING.lock().map_err(|_| "busy".to_string())?;
    // Starting again on a port replaces what held it: the old server is stopped, so a port is on the list once.
    running.retain_mut(|server| {
        if server.key != port {
            return true;
        }
        kill_tree(&mut server.child);
        false
    });
    running.push(Server { key: port, child, command: command.to_string(), cwd: cwd.to_string(), url: None, started: std::time::SystemTime::now() });
    Ok(log)
}

/// Whether the server this app started for `port` has already exited (a failed start).
pub fn exited(port: u16) -> bool {
    let Ok(mut running) = RUNNING.lock() else {
        return false;
    };
    running
        .iter_mut()
        .filter(|server| server.key == port)
        .all(|server| !matches!(server.child.try_wait(), Ok(None)))
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

/// Stops a server with the processes it started (a dev server is a shell, a package manager and a node process:
/// killing only the first would leave the others listening).
fn kill_tree(child: &mut Child) {
    let id = child.id().to_string();
    if cfg!(windows) {
        let mut kill = Command::new("taskkill");
        kill.args(["/PID", &id, "/T", "/F"]).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            kill.creation_flags(0x0800_0000);
        }
        let _ = kill.status();
    } else {
        let _ = Command::new("kill").args(["-TERM", &format!("-{id}")]).status();
    }
    let _ = child.kill();
}

/// Stops every server this app started.
pub fn kill_all() {
    let Ok(mut running) = RUNNING.lock() else { return };
    for server in running.iter_mut() {
        kill_tree(&mut server.child);
    }
    running.clear();
}

/// Remembers the address a server came up at, for the list of what runs.
pub fn set_url(port: u16, url: &str) {
    if let Ok(mut running) = RUNNING.lock() {
        for server in running.iter_mut().filter(|server| server.key == port) {
            server.url = Some(url.to_string());
        }
    }
}

/// A server in the list of what runs in the background.
#[derive(serde::Serialize)]
pub struct ServerInfo {
    pub key: u16,
    pub command: String,
    pub cwd: String,
    pub url: Option<String>,
    pub pid: u32,
    /// Seconds since it started.
    pub age: u64,
    /// False once it has exited (a crash, or it stopped by itself): the entry stays until it is cleared.
    pub running: bool,
}

/// How long a server may stay without answering on its port before it is stopped.
const NO_ANSWER_SECS: u64 = 300;

#[tauri::command]
pub fn servers_list() -> Vec<ServerInfo> {
    let Ok(mut running) = RUNNING.lock() else { return Vec::new() };
    // A server that never answered on its port is stopped after a while: it is stuck, or it uses another port.
    for server in running.iter_mut() {
        let waited = server.started.elapsed().map_or(0, |age| age.as_secs());
        if server.url.is_none() && waited > NO_ANSWER_SECS && matches!(server.child.try_wait(), Ok(None)) {
            kill_tree(&mut server.child);
        }
    }
    running
        .iter_mut()
        .map(|server| ServerInfo {
            key: server.key,
            command: server.command.clone(),
            cwd: server.cwd.clone(),
            url: server.url.clone(),
            pid: server.child.id(),
            age: server.started.elapsed().map_or(0, |age| age.as_secs()),
            running: matches!(server.child.try_wait(), Ok(None)),
        })
        .collect()
}

/// Stops the server on `key` (and takes it off the list); `None` stops all of them.
#[tauri::command]
pub fn server_stop(key: Option<u16>) -> Result<(), String> {
    let mut running = RUNNING.lock().map_err(|_| "busy".to_string())?;
    for server in running.iter_mut().filter(|server| key.is_none_or(|key| server.key == key)) {
        kill_tree(&mut server.child);
    }
    running.retain(|server| key.is_some_and(|key| server.key != key));
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn pings() -> usize {
        let out = Command::new("tasklist").args(["/FI", "IMAGENAME eq PING.EXE", "/NH"]).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        out.lines().filter(|line| line.to_uppercase().contains("PING.EXE")).count()
    }

    #[test]
    fn kill_all_ends_the_server_and_the_processes_it_started() {
        let before = pings();
        let cwd = std::env::temp_dir();
        start("ping -n 60 127.0.0.1", &cwd.to_string_lossy(), 59_998).expect("starts");
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(pings() > before, "the child of the shell is running");
        kill_all();
        std::thread::sleep(std::time::Duration::from_millis(800));
        assert_eq!(pings(), before, "no process of the server is left");
        assert!(exited(59_998));
    }
}
