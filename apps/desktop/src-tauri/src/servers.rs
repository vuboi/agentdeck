//! Dev servers AgentDeck runs for the agent (`browser_serve`). They belong to the app, not to the agent's shell:
//! a server the CLI starts inside one of its commands dies when that command or the turn ends, and the Preview
//! is left on a refused connection. Output goes to `~/.agentdeck/servers/<port>.log`.
//!
//! They also end with the app: on Windows every server joins a job object that kills its processes when the app
//! goes away (even if the app crashes); on exit every one is stopped with its children as well.

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Mutex, OnceLock};

#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// The raw job handle (0: it could not be made, and servers then only stop through `kill_all`).
    static JOB: OnceLock<isize> = OnceLock::new();

    fn handle() -> Option<HANDLE> {
        let raw = *JOB.get_or_init(|| {
            // SAFETY: plain Win32 calls with a handle made here and a correctly sized, initialised struct.
            unsafe {
                let Ok(job) = CreateJobObjectW(None, None) else {
                    return 0;
                };
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
        adopt_handle(child.as_raw_handle());
    }

    /// The same for an agent CLI (a tokio child, see `ag_adapters::agent::runner::on_spawn`): killing the app ends it too.
    pub fn adopt_agent(child: &tokio::process::Child) {
        if let Some(raw) = child.raw_handle() {
            adopt_handle(raw);
        }
    }

    fn adopt_handle(raw: std::os::windows::io::RawHandle) {
        if let Some(job) = handle() {
            // SAFETY: both handles are valid: the job lives for the whole run, the child is still owned by the caller.
            let _ = unsafe { AssignProcessToJobObject(job, HANDLE(raw)) };
        }
    }
}

/// Ties every agent CLI to the app's job (Windows), so a killed app leaves none running. Elsewhere a CLI leads its own
/// process group and ends with its turn.
pub fn adopt_agents() {
    #[cfg(windows)]
    ag_adapters::agent::runner::on_spawn(job::adopt_agent);
}

/// What tells the page that the log of the server on a port grew (set once at start-up; tests run without one). A
/// callback, not the app handle, so this module (and its tests) does not pull the window toolkit in.
type LogAnnouncer = Box<dyn Fn(u16) + Send + Sync>;
static ANNOUNCE: OnceLock<LogAnnouncer> = OnceLock::new();

/// Gives the module the function that announces a grown log.
pub fn init(announce: impl Fn(u16) + Send + Sync + 'static) {
    let _ = ANNOUNCE.set(Box::new(announce));
}

/// How often a running server's log file is looked at for growth. A look is one `stat`; the page is told only when
/// the length changed, so a quiet server costs the webview nothing.
const LOG_LOOK: std::time::Duration = std::time::Duration::from_millis(250);

/// Whether the log is now a different length than at the last look (it grew, or the file was started over).
fn changed(last: &mut u64, now: u64) -> bool {
    let differs = *last != now;
    *last = now;
    differs
}

/// Emits `server-log` {port} whenever the log of the server (process `pid`) on `port` changes, until that server has
/// ended or been replaced; one last event goes out when it ends, so the page reads the final lines.
fn watch_log(port: u16, pid: u32) {
    let Some(announce) = ANNOUNCE.get() else {
        return;
    };
    let Ok(path) = log_path(port) else { return };
    std::thread::spawn(move || {
        let mut last = 0u64;
        loop {
            std::thread::sleep(LOG_LOOK);
            let length = std::fs::metadata(&path).map_or(0, |meta| meta.len());
            if changed(&mut last, length) {
                announce(port);
            }
            let alive = RUNNING
                .lock()
                .map(|mut running| {
                    running.iter_mut().any(|server| {
                        server.key == port
                            && server.child.id() == pid
                            && matches!(server.child.try_wait(), Ok(None))
                    })
                })
                .unwrap_or(false);
            if !alive {
                announce(port);
                return;
            }
        }
    });
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
    Ok(crate::home_folder()?
        .join(".agentdeck")
        .join("servers")
        .join(format!("{port}.log")))
}

/// How a server is started besides its command.
#[derive(Clone, Default, Debug)]
pub struct Options {
    /// The script may ask questions: it gets a keyboard, and its questions are answered (see `answers.rs`).
    pub interactive: bool,
    /// What to answer to the questions, in the order they come. A number, or words that name a menu option.
    pub answers: Vec<String>,
    /// Added to the environment (`PORT` when the usual port is taken).
    pub env: Vec<(String, String)>,
}

/// Whether the server this app started on `port` is still running.
pub fn runs_on(port: u16) -> bool {
    RUNNING
        .lock()
        .map(|mut running| {
            running
                .iter_mut()
                .any(|server| server.key == port && matches!(server.child.try_wait(), Ok(None)))
        })
        .unwrap_or(false)
}

/// The command and port to use when the expected `port` is taken by a program that is not the page we want: the next
/// free port, in the command (`--port 4200`) and in `PORT`. Unchanged when the port is free, or this app's own server
/// is the one holding it.
pub fn prepare_port(command: &str, port: u16) -> (String, u16, Vec<(String, String)>) {
    if crate::answers::port_free(port) || runs_on(port) {
        return (command.to_string(), port, Vec::new());
    }
    let next = crate::answers::free_port(port.saturating_add(1));
    (
        crate::answers::with_port(command, port, next),
        next,
        vec![("PORT".to_string(), next.to_string())],
    )
}

/// How many answers a script gets before it is left alone (a loop of questions is not ours to answer for ever).
const MAX_ANSWERS: usize = 12;
/// How long the log stays unchanged before its last lines are taken for a question waiting for an answer.
const QUIET: std::time::Duration = std::time::Duration::from_millis(700);

/// The log of the server on `port` from byte `from` on, without colour codes.
fn log_since(port: u16, from: u64) -> (String, u64) {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(path) = log_path(port) else { return (String::new(), from) };
    let Ok(mut file) = std::fs::File::open(path) else { return (String::new(), from) };
    let length = file.metadata().map_or(0, |meta| meta.len());
    let _ = file.seek(SeekFrom::Start(from.min(length)));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    (crate::detect::plain(&String::from_utf8_lossy(&bytes)), length)
}

/// Types the answers a script's questions need, while the server (process `pid` on `port`) runs.
fn answer_questions(port: u16, pid: u32, mut keyboard: ChildStdin, options: Options, expected: u16) {
    std::thread::spawn(move || {
        let mut queue: VecDeque<String> = options.answers.iter().cloned().collect();
        let (mut answered, mut seen) = (0u64, 0u64);
        let mut changed_at = std::time::Instant::now();
        let mut given = 0;
        loop {
            std::thread::sleep(LOG_LOOK);
            let alive = RUNNING
                .lock()
                .map(|mut running| {
                    running.iter_mut().any(|server| {
                        server.key == port && server.child.id() == pid && matches!(server.child.try_wait(), Ok(None))
                    })
                })
                .unwrap_or(false);
            if !alive {
                return;
            }
            let length = log_path(port)
                .ok()
                .and_then(|path| std::fs::metadata(path).ok())
                .map_or(0, |meta| meta.len());
            if length != seen {
                seen = length;
                changed_at = std::time::Instant::now();
                continue;
            }
            if length <= answered || changed_at.elapsed() < QUIET {
                continue;
            }
            let (text, end) = log_since(port, answered);
            // Output that is not a question (a build, a banner) is left alone; the script may still be working.
            if !crate::detect::looks_interactive(&text) {
                continue;
            }
            let answer = crate::answers::decide(&text, &mut queue, Some(expected));
            if writeln!(keyboard, "{answer}").and_then(|()| keyboard.flush()).is_err() {
                return;
            }
            answered = end;
            given += 1;
            if given >= MAX_ANSWERS {
                return;
            }
        }
    });
}

/// Starts `command` in `cwd` for the server expected on `port`, with `options`: a keyboard and answers for a script
/// that asks, and extra environment. A server this app started earlier on the same port is stopped first.
pub fn start_with(command: &str, cwd: &str, port: u16, options: &Options) -> Result<PathBuf, String> {
    let log = log_path(port)?;
    std::fs::create_dir_all(log.parent().ok_or("no folder for the server log")?)
        .map_err(|error| error.to_string())?;
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
    shell
        .current_dir(cwd)
        .envs(options.env.iter().cloned())
        .stdin(if options.interactive { Stdio::piped() } else { Stdio::null() })
        .stdout(out)
        .stderr(err);
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
    let mut child = shell
        .spawn()
        .map_err(|error| format!("could not start `{command}` in {cwd}: {error}"))?;
    let keyboard = child.stdin.take();
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
    let pid = child.id();
    running.push(Server {
        key: port,
        child,
        command: command.to_string(),
        cwd: cwd.to_string(),
        url: None,
        started: std::time::SystemTime::now(),
    });
    drop(running);
    watch_log(port, pid);
    if let Some(keyboard) = keyboard {
        answer_questions(port, pid, keyboard, options.clone(), port);
    }
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

/// The end of a server's log as it is (up to 64 KB, colour codes and all).
pub fn read_log(port: u16) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(path) = log_path(port) else {
        return String::new();
    };
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let length = file.metadata().map_or(0, |meta| meta.len());
    let _ = file.seek(SeekFrom::Start(length.saturating_sub(64 * 1024)));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Stops a server with the processes it started (a dev server is a shell, a package manager and a node process:
/// killing only the first would leave the others listening).
fn kill_tree(child: &mut Child) {
    let id = child.id().to_string();
    if cfg!(windows) {
        let mut kill = Command::new("taskkill");
        kill.args(["/PID", &id, "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            kill.creation_flags(0x0800_0000);
        }
        let _ = kill.status();
    } else {
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{id}")])
            .status();
    }
    let _ = child.kill();
}

/// Stops every server this app started.
pub fn kill_all() {
    let Ok(mut running) = RUNNING.lock() else {
        return;
    };
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
    let Ok(mut running) = RUNNING.lock() else {
        return Vec::new();
    };
    // A server that never answered on its port is stopped after a while: it is stuck, or it uses another port.
    for server in running.iter_mut() {
        let waited = server.started.elapsed().map_or(0, |age| age.as_secs());
        if server.url.is_none()
            && waited > NO_ANSWER_SECS
            && matches!(server.child.try_wait(), Ok(None))
        {
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
    for server in running
        .iter_mut()
        .filter(|server| key.is_none_or(|key| server.key == key))
    {
        kill_tree(&mut server.child);
    }
    running.retain(|server| key.is_some_and(|key| server.key != key));
    Ok(())
}

#[cfg(test)]
mod log_tests {
    use super::*;

    #[test]
    fn a_log_is_announced_only_when_its_length_changes() {
        let mut last = 0;
        assert!(!changed(&mut last, 0));
        assert!(changed(&mut last, 120));
        assert!(!changed(&mut last, 120));
        // Started over (a server restarted on the same port): shorter, still a change.
        assert!(changed(&mut last, 10));
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// Both tests stop every server (`kill_all`): one at a time, or one would stop the other's.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    fn pings() -> usize {
        let out = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq PING.EXE", "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        out.lines()
            .filter(|line| line.to_uppercase().contains("PING.EXE"))
            .count()
    }

    #[test]
    fn kill_all_ends_the_server_and_the_processes_it_started() {
        let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = pings();
        let cwd = std::env::temp_dir();
        start_with("ping -n 60 127.0.0.1", &cwd.to_string_lossy(), 59_998, &Options::default()).expect("starts");
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(pings() > before, "the child of the shell is running");
        kill_all();
        std::thread::sleep(std::time::Duration::from_millis(800));
        assert_eq!(pings(), before, "no process of the server is left");
        assert!(exited(59_998));
    }

    /// A script that asks which project and which port starts by itself: the questions are answered from the log.
    #[test]
    fn a_script_that_asks_is_answered() {
        let _alone = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let folder = std::env::temp_dir().join("agentdeck-test-asks");
        std::fs::create_dir_all(&folder).expect("a folder");
        std::fs::write(
            folder.join("ask.js"),
            r#"const rl=require('readline').createInterface({input:process.stdin,output:process.stdout});
rl.question('Which app?\n1) web admin\n2) web shop\nNumber: ',a=>{
  rl.question('Port (default 4200): ',p=>{console.log('GOT '+a+' '+p);process.exit(0)});
});"#,
        )
        .expect("a script");
        let options = Options { interactive: true, answers: vec!["shop".to_string()], env: Vec::new() };
        start_with("node ask.js", &folder.to_string_lossy(), 59_997, &options).expect("starts");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut log = String::new();
        while std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(300));
            log = read_log(59_997);
            if log.contains("GOT ") {
                break;
            }
        }
        kill_all();
        assert!(log.contains("GOT 2 "), "the shop was chosen, then a port: {log}");
    }
}
