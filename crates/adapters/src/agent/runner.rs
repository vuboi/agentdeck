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
use super::{AgentEvent, Provider, TurnRequest};

/// How long output may still come in after the CLI exited. A process it left running (a dev server started in the
/// background) can hold the pipe open for ever, so the stream is not read to its end.
const EXIT_GRACE: Duration = Duration::from_secs(2);

/// The last part of stderr kept for the error message.
const STDERR_KEEP: usize = 16 * 1024;

/// Reads stderr while the CLI runs, keeping only its tail. A pipe nobody reads fills up (4 KB on Windows) and the
/// CLI then blocks on its next write, so the turn would never end.
async fn drain_stderr(mut stderr: tokio::process::ChildStderr, kept: Arc<Mutex<Vec<u8>>>) {
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
fn command_for(binary: OsString, args: &[String]) -> Command {
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
                },
            };
            let Ok(Some(bytes)) = next else { break };
            let text = String::from_utf8_lossy(&bytes);
            let line = text.strip_suffix('\r').unwrap_or(&text);
            for event in provider.parse_line(line) {
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
