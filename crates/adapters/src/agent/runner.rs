//! Runs one agent turn: starts the CLI, reads its output line by line and turns each line into events.
//! Kept apart from the app so the process handling is tested with stand-in binaries and runs the same on
//! Windows, macOS and Linux. Finding the binary is in [`super::resolve`].

use std::ffi::OsString;
#[cfg(windows)]
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;

pub use super::resolve::resolve_binary;
use super::resolve::augmented_path;
use super::stall::Watchdog;
use super::{AgentEvent, Provider, TurnRequest};

/// How long output may still come in after the CLI exited. A process it left running (a dev server started in the
/// background) can hold the pipe open for ever, so the stream is not read to its end.
const EXIT_GRACE: Duration = Duration::from_secs(2);

/// The last part of stderr kept for the error message.
const STDERR_KEEP: usize = 16 * 1024;

/// Reads stderr while the CLI runs, keeping only its tail. A pipe nobody reads fills up (4 KB on Windows) and the
/// CLI then blocks on its next write, so the turn would never end.
pub(super) async fn drain_stderr(mut stderr: tokio::process::ChildStderr, kept: Arc<Mutex<Vec<u8>>>) {
    let mut chunk = [0u8; 4096];
    while let Ok(read) = stderr.read(&mut chunk).await {
        if read == 0 {
            break;
        }
        let Ok(mut kept) = kept.lock() else { break };
        kept.extend_from_slice(&chunk[..read]);
        if kept.len() > STDERR_KEEP {
            let extra = kept.len() - STDERR_KEEP;
            kept.drain(..extra);
        }
    }
}

/// Windows cannot start a `.cmd`/`.bat` file through `CreateProcess` with arguments directly.
pub(super) fn command_for(binary: OsString, args: &[String]) -> Command {
    #[cfg(windows)]
    {
        let is_batch = Path::new(&binary)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            });
        if is_batch {
            let mut command = Command::new("cmd");
            command.args(["/D", "/S", "/C"]).arg(binary).args(args);
            return command;
        }
    }
    let mut command = Command::new(binary);
    command.args(args);
    command
}

/// Called with every agent CLI the crate starts, right after it starts (the app ties it to its own lifetime: on Windows a
/// job object that ends it, and what it started, when the app ends, even when the app is killed and drops run no more).
type SpawnHook = fn(&tokio::process::Child);
static ON_SPAWN: std::sync::OnceLock<SpawnHook> = std::sync::OnceLock::new();

/// Sets the function told of each CLI started (once, at start-up).
pub fn on_spawn(hook: SpawnHook) {
    let _ = ON_SPAWN.set(hook);
}

pub(super) fn spawned(child: &tokio::process::Child) {
    if let Some(hook) = ON_SPAWN.get() {
        hook(child);
    }
}

/// Stops the whole process tree of a turn that is dropped before it ends (the user stopped it, or sent an edited
/// message). `kill_on_drop` ends only the process the app started: on Windows that is the `cmd /C` around a `.cmd`
/// CLI, and everywhere a CLI started by a wrapper (an npm shim, `node`) keeps its own children. They kept running,
/// still holding the session, and the next turn resuming that session hung.
struct TreeGuard(Option<u32>);

impl TreeGuard {
    fn disarm(&mut self) {
        self.0 = None;
    }
}

impl Drop for TreeGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            kill_tree(pid);
        }
    }
}

/// Ends a process and everything it started. Fire and forget: the kill runs on its own. The one place that does this:
/// the app's own cleanup (`process_guard`, the image generator) calls it too.
pub fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .spawn();
    }
    // macOS and Linux: the CLI leads a process group of its own (see `drive_turn`), so the group is ended.
    #[cfg(unix)]
    {
        let _ = std::process::Command::new("sh")
            .args(["-c", &kill_script(pid)])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

/// The shell lines that end a process, its group and its direct children, then the process itself in case it does not lead
/// a group. Pure, so it is tested on any system (and built on every system, for the app's tests).
pub fn kill_script(pid: u32) -> String {
    format!("kill -KILL -- -{pid} 2>/dev/null; pkill -KILL -P {pid} 2>/dev/null; kill -KILL {pid} 2>/dev/null; exit 0")
}

/// Runs one turn: start the CLI, read its stream line by line, translate each line, hand each event to `emit`.
/// Separate from Tauri so the process handling can be tested with a stand-in binary.
pub async fn drive_turn(
    binary: std::ffi::OsString,
    provider: &dyn Provider,
    req: &TurnRequest,
    mut emit: impl FnMut(AgentEvent),
) {
    if let Err(message) = provider.prepare(req) {
        return emit(AgentEvent::Error { message });
    }
    let args = provider.args(req);
    let mut command = command_for(binary, &args);
    command
        .envs(provider.environment(req))
        .stdin(if provider.prompt_on_stdin() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(path) = augmented_path() {
        command.env("PATH", path);
    }
    // A folder that is gone (or an empty path, for chats without a project) would make the spawn itself fail.
    if let Some(dir) = req.cwd.as_deref().filter(|dir| dir.is_dir()) {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashing up per turn
    // Its own process group, so a stop ends what it started too (TreeGuard).
    #[cfg(unix)]
    command.process_group(0);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let hint = if provider.id() == "opencode" {
                " Open Settings → Connectors → Install OpenCode, then retry."
            } else {
                " Check that its CLI is installed in Settings → Connectors."
            };
            return emit(AgentEvent::Error {
                message: format!("could not start {}: {error}.{hint}", provider.binary()),
            });
        }
    };
    spawned(&child);
    let mut tree = TreeGuard(child.id());

    // Written apart from the reader: a long prompt must not hold up the output, and the CLI reads all of it
    // before it starts working, so a closed or early-exiting child only ends this write.
    if let Some(mut stdin) = child.stdin.take() {
        let prompt = req.prompt.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(prompt.as_bytes()).await;
            let _ = stdin.shutdown().await;
        });
    }

    let stderr_kept = Arc::new(Mutex::new(Vec::new()));
    let stderr = child
        .stderr
        .take()
        .map(|err| tokio::spawn(drain_stderr(err, Arc::clone(&stderr_kept))));

    let mut finished = false;
    let mut stalled = false;
    // A sub-agent is still open, and the result the CLI printed before it ended (kept, in case nothing follows).
    let mut agents_open = false;
    let mut early_done: Option<AgentEvent> = None;
    let mut watchdog = Watchdog::new(provider.stall_limit());
    // Set once the CLI has exited, with the time after which its pipe is no longer read.
    let mut exited: Option<(std::io::Result<std::process::ExitStatus>, tokio::time::Instant)> = None;
    if let Some(stdout) = child.stdout.take() {
        // Split on bytes, not `lines()`: one line that is not UTF-8 would end that reader with an error and leave
        // the CLI blocked on a pipe nobody reads.
        let mut lines = BufReader::new(stdout).split(b'\n');
        loop {
            let next = match &exited {
                Some((_, deadline)) => tokio::time::timeout_at(*deadline, lines.next_segment())
                    .await
                    .unwrap_or(Ok(None)),
                None => tokio::select! {
                    next = lines.next_segment() => next,
                    status = child.wait() => {
                        exited = Some((status, tokio::time::Instant::now() + EXIT_GRACE));
                        continue;
                    }
                    // Alive but silent for too long: end the turn and the whole tree.
                    _ = tokio::time::sleep_until(watchdog.deadline()) => {
                        emit(AgentEvent::Error { message: watchdog.message() });
                        finished = true;
                        stalled = true;
                        break;
                    }
                },
            };
            let Ok(Some(bytes)) = next else { break };
            watchdog.touch(); // any line, even one nobody understands, shows the CLI is alive
            let text = String::from_utf8_lossy(&bytes);
            let line = text.strip_suffix('\r').unwrap_or(&text);
            if let Some(open) = super::tail::agents_open(line) {
                agents_open = open;
                watchdog.set_background(open);
            }
            for event in provider.parse_line(line) {
                watchdog.note(&event);
                // A worker is asked for the answer of the work it handed to sub-agents: the CLI's first result comes
                // while they still run, and it goes on by itself when they end. That second result is the answer.
                if agents_open && matches!(event, AgentEvent::Done { .. }) {
                    early_done = Some(event);
                    continue;
                }
                finished |= matches!(event, AgentEvent::Done { .. } | AgentEvent::Error { .. });
                emit(event);
            }
            // The turn is over once its result is out. The CLI may still hold the pipe open (a dev server it
            // started in the background does), so do not wait for the stream to end.
            if finished {
                break;
            }
        }
    }

    // The CLI ended (or went silent for good) without a second result: the first one is the answer.
    if !finished {
        if let Some(done) = early_done.take() {
            finished = true;
            emit(done);
        }
    }
    // Ended on its own: what the CLI left running in the background (a dev server) is not ours to stop.
    // A stalled turn is the exception: the guard ends the whole tree when it drops.
    if !stalled {
        tree.disarm();
    }
    let status = match exited {
        Some((status, _)) => status,
        None => {
            if finished {
                let _ = child.start_kill();
            }
            child.wait().await
        }
    };
    if !finished {
        if let Some(event) = provider.finish(req) {
            emit(event);
            return;
        }
        // A process the CLI left running can hold stderr open too: what was read by then is enough.
        if let Some(task) = stderr {
            let abort = task.abort_handle();
            if tokio::time::timeout(EXIT_GRACE, task).await.is_err() {
                abort.abort();
            }
        }
        let stderr = stderr_kept
            .lock()
            .map(|kept| String::from_utf8_lossy(&kept).into_owned())
            .unwrap_or_default();
        let message = match status {
            Ok(status) if status.success() => "agent ended without a result".to_string(),
            _ => stderr.trim().to_string(),
        };
        emit(AgentEvent::Error {
            message: if message.is_empty() {
                "agent failed".into()
            } else {
                message
            },
        });
    }
}

#[cfg(all(test, unix))] // the fake agents are sh scripts
mod unix_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A provider whose "CLI" is a shell script that prints the given lines and exits with `code`.
    struct Fake;
    impl Provider for Fake {
        fn id(&self) -> &'static str {
            "fake"
        }
        fn binary(&self) -> &'static str {
            "fake"
        }
        fn candidates(&self) -> Vec<std::path::PathBuf> {
            Vec::new()
        }
        fn args(&self, _req: &TurnRequest) -> Vec<String> {
            Vec::new()
        }
        fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
            super::super::claude::Claude.parse_line(line)
        }
    }

    fn script(name: &str, body: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("agentdeck-test-{name}-{}.sh", std::process::id()));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn req() -> TurnRequest {
        TurnRequest {
            prompt: "hi".into(),
            session_id: None,
            resume: false,
            model: None,
            permission_mode: None,
            effort: None,
            allowed_tools: Vec::new(),
            cwd: None,
            owner: None,
            api_key: None,
        }
    }

    #[tokio::test]
    async fn streamed_lines_become_events_in_order() {
        let path = script(
            "ok",
            r#"echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"po"}}}'
echo '{"type":"result","is_error":false,"result":"pong","session_id":"s1"}'"#,
        );
        let mut events = Vec::new();
        drive_turn(path.clone().into_os_string(), &Fake, &req(), |e| {
            events.push(e)
        })
        .await;
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            events,
            vec![
                AgentEvent::Delta { text: "po".into() },
                AgentEvent::Done {
                    session_id: Some("s1".into()),
                    text: "pong".into(),
                    cost_usd: None
                },
            ]
        );
    }

    #[tokio::test]
    async fn failure_without_result_reports_stderr() {
        let path = script("fail", "echo 'boom' >&2\nexit 3");
        let mut events = Vec::new();
        drive_turn(path.clone().into_os_string(), &Fake, &req(), |e| {
            events.push(e)
        })
        .await;
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            events,
            vec![AgentEvent::Error {
                message: "boom".into()
            }]
        );
    }

    #[tokio::test]
    async fn heavy_stderr_does_not_block_the_turn() {
        // Far more stderr than a pipe holds, written before the result.
        let path = script(
            "noisy",
            r#"i=0; while [ $i -lt 4000 ]; do echo 'warning: something the CLI logs on stderr' >&2; i=$((i+1)); done
echo '{"type":"result","is_error":false,"result":"pong","session_id":"s1"}'"#,
        );
        let mut events = Vec::new();
        let run = drive_turn(path.clone().into_os_string(), &Fake, &req(), |e| {
            events.push(e)
        });
        let ended = tokio::time::timeout(Duration::from_secs(20), run).await;
        let _ = std::fs::remove_file(&path);
        assert!(ended.is_ok(), "the turn hung on a full stderr pipe");
        assert!(matches!(events.last(), Some(AgentEvent::Done { .. })));
    }

    #[tokio::test]
    async fn exit_ends_the_turn_while_a_left_process_holds_the_pipe() {
        // The CLI dies without a result; a process it started keeps stdout and stderr open.
        let path = script("orphan", "sleep 30 &\necho 'boom' >&2\nexit 3");
        let mut events = Vec::new();
        let run = drive_turn(path.clone().into_os_string(), &Fake, &req(), |e| {
            events.push(e)
        });
        let ended = tokio::time::timeout(Duration::from_secs(10), run).await;
        let _ = std::fs::remove_file(&path);
        assert!(ended.is_ok(), "the turn waited for the left process");
        assert_eq!(
            events,
            vec![AgentEvent::Error {
                message: "boom".into()
            }]
        );
    }

    #[tokio::test]
    async fn a_line_that_is_not_utf8_does_not_end_the_stream() {
        let path = script(
            "latin1",
            r#"printf '\377\376 not utf-8\n'
echo '{"type":"result","is_error":false,"result":"pong","session_id":"s1"}'"#,
        );
        let mut events = Vec::new();
        drive_turn(path.clone().into_os_string(), &Fake, &req(), |e| {
            events.push(e)
        })
        .await;
        let _ = std::fs::remove_file(&path);
        assert!(matches!(events.last(), Some(AgentEvent::Done { .. })));
    }

    /// `Fake` with a short stall limit, so the watchdog can be seen working in a second.
    struct Impatient;
    impl Provider for Impatient {
        fn id(&self) -> &'static str {
            "impatient"
        }
        fn binary(&self) -> &'static str {
            "impatient"
        }
        fn candidates(&self) -> Vec<std::path::PathBuf> {
            Vec::new()
        }
        fn args(&self, _req: &TurnRequest) -> Vec<String> {
            Vec::new()
        }
        fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
            super::super::claude::Claude.parse_line(line)
        }
        fn stall_limit(&self) -> Duration {
            Duration::from_secs(1)
        }
    }

    #[tokio::test]
    async fn a_silent_cli_is_ended_with_a_clear_error() {
        let path = script("silent", "sleep 30");
        let mut events = Vec::new();
        let run = drive_turn(path.clone().into_os_string(), &Impatient, &req(), |e| {
            events.push(e)
        });
        let ended = tokio::time::timeout(Duration::from_secs(10), run).await;
        let _ = std::fs::remove_file(&path);
        assert!(ended.is_ok(), "the watchdog did not end the turn");
        assert_eq!(
            events,
            vec![AgentEvent::Error {
                message: "agent produced no output for 1 s".into()
            }]
        );
    }

    #[tokio::test]
    async fn any_output_line_resets_the_watchdog() {
        // 5 lines 0.4 s apart: 2 s in all, over the 1 s limit, but never silent for 1 s.
        let path = script(
            "slow",
            r#"for i in 1 2 3 4 5; do echo 'not json, but alive'; sleep 0.4; done
echo '{"type":"result","is_error":false,"result":"pong","session_id":"s1"}'"#,
        );
        let mut events = Vec::new();
        drive_turn(path.clone().into_os_string(), &Impatient, &req(), |e| {
            events.push(e)
        })
        .await;
        let _ = std::fs::remove_file(&path);
        assert!(matches!(events.last(), Some(AgentEvent::Done { .. })), "{events:?}");
    }

    #[tokio::test]
    async fn missing_binary_reports_an_error() {
        let mut events = Vec::new();
        drive_turn("/nonexistent/agent-binary".into(), &Fake, &req(), |e| {
            events.push(e)
        })
        .await;
        assert!(matches!(events.as_slice(), [AgentEvent::Error { .. }]));
    }
}

#[cfg(test)]
mod kill_tests {
    use super::kill_script;

    #[test]
    fn the_kill_script_ends_the_group_the_children_and_the_process() {
        let script = kill_script(4242);
        assert!(script.contains("kill -KILL -- -4242"));
        assert!(script.contains("pkill -KILL -P 4242"));
        assert!(script.contains("kill -KILL 4242"));
        assert!(script.ends_with("exit 0"));
    }
}

#[cfg(test)]
mod worker_tests {
    use super::*;
    use crate::agent::claude::Claude;

    /// A Claude-like provider whose "CLI" is `sh script`.
    struct Fake(String);
    impl Provider for Fake {
        fn id(&self) -> &'static str {
            "fakeworker"
        }
        fn binary(&self) -> &'static str {
            "sh"
        }
        fn candidates(&self) -> Vec<std::path::PathBuf> {
            Vec::new()
        }
        fn args(&self, _req: &TurnRequest) -> Vec<String> {
            vec![self.0.clone()]
        }
        fn parse_line(&self, line: &str) -> Vec<AgentEvent> {
            Claude.parse_line(line)
        }
    }

    fn request() -> TurnRequest {
        TurnRequest {
            prompt: "go".into(),
            session_id: None,
            resume: false,
            model: None,
            permission_mode: None,
            effort: None,
            allowed_tools: Vec::new(),
            cwd: None,
            owner: None,
            api_key: None,
        }
    }

    async fn run(name: &str, body: &str) -> Vec<AgentEvent> {
        let path = std::env::temp_dir().join(format!("agentdeck-worker-{name}-{}.sh", std::process::id()));
        std::fs::write(&path, body).unwrap();
        let provider = Fake(path.to_string_lossy().replace('\\', "/"));
        let mut events = Vec::new();
        drive_turn("sh".into(), &provider, &request(), |event| events.push(event)).await;
        let _ = std::fs::remove_file(&path);
        events
    }

    #[tokio::test]
    async fn a_worker_waits_for_its_sub_agents_and_answers_with_the_second_result() {
        let events = run(
            "agents",
            r#"echo '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"a","task_type":"local_agent"}]}'
echo '{"type":"result","subtype":"success","session_id":"s1","result":"started"}'
sleep 1
echo '{"type":"system","subtype":"background_tasks_changed","tasks":[]}'
echo '{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"the real answer"}}}'
echo '{"type":"result","subtype":"success","session_id":"s1","result":"the real answer"}'
"#,
        )
        .await;
        let dones: Vec<&AgentEvent> = events.iter().filter(|event| matches!(event, AgentEvent::Done { .. })).collect();
        assert_eq!(dones.len(), 1, "{events:?}");
        assert!(matches!(dones[0], AgentEvent::Done { text, .. } if text == "the real answer"), "{events:?}");
    }

    #[tokio::test]
    async fn a_worker_does_not_wait_for_a_shell_left_running() {
        let events = run(
            "shell",
            r#"echo '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"b","task_type":"local_bash"}]}'
echo '{"type":"result","subtype":"success","session_id":"s1","result":"started the server"}'
sleep 30
"#,
        )
        .await;
        assert!(matches!(events.last(), Some(AgentEvent::Done { text, .. }) if text == "started the server"), "{events:?}");
    }

    #[tokio::test]
    async fn a_worker_whose_cli_ends_after_the_first_result_still_answers() {
        let events = run(
            "early",
            r#"echo '{"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"a","task_type":"local_agent"}]}'
echo '{"type":"result","subtype":"success","session_id":"s1","result":"only answer"}'
"#,
        )
        .await;
        assert!(matches!(events.last(), Some(AgentEvent::Done { text, .. }) if text == "only answer"), "{events:?}");
    }
}
