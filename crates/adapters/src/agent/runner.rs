//! Runs one agent turn: finds the CLI, starts it, reads its output line by line and turns each line into events.
//! Kept apart from the app so the process handling is tested with stand-in binaries and runs the same on
//! Windows, macOS and Linux.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use super::{AgentEvent, Provider, TurnRequest};

/// File extensions a bare command name may carry on this platform.
fn executable_suffixes(os: &str) -> &'static [&'static str] {
    if os == "windows" {
        &[".exe", ".cmd", ".bat"]
    } else {
        &[""]
    }
}

/// First existing `<dir>/<name><suffix>` for the directories of a PATH-like list.
fn find_in(dirs: impl Iterator<Item = PathBuf>, name: &str, os: &str) -> Option<PathBuf> {
    dirs.flat_map(|dir| {
        executable_suffixes(os)
            .iter()
            .map(move |suffix| dir.join(format!("{name}{suffix}")))
            .collect::<Vec<_>>()
    })
    .find(|path| path.is_file())
}

/// The binary to run: the provider's own well-known locations first (a desktop app often starts without
/// the shell PATH, notably on macOS), then PATH, then the bare name so the OS can report "not found".
pub fn resolve_binary(provider: &dyn Provider) -> OsString {
    provider
        .candidates()
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| {
            let path = std::env::var_os("PATH")?;
            find_in(
                std::env::split_paths(&path),
                provider.binary(),
                std::env::consts::OS,
            )
        })
        .map(PathBuf::into_os_string)
        .unwrap_or_else(|| provider.binary().into())
}

/// Runs one turn: start the CLI, read its stream line by line, translate each line, hand each event to `emit`.
/// Separate from Tauri so the process handling can be tested with a stand-in binary.
pub async fn drive_turn(
    binary: std::ffi::OsString,
    provider: &dyn Provider,
    req: &TurnRequest,
    mut emit: impl FnMut(AgentEvent),
) {
    let mut command = Command::new(binary);
    command
        .args(provider.args(req))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // A folder that is gone (or an empty path, for chats without a project) would make the spawn itself fail.
    if let Some(dir) = req.cwd.as_deref().filter(|dir| dir.is_dir()) {
        command.current_dir(dir);
    }
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashing up per turn
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return emit(AgentEvent::Error {
                message: format!("could not start {}: {error}", provider.binary()),
            });
        }
    };

    let mut finished = false;
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            for event in provider.parse_line(&line) {
                finished |= matches!(event, AgentEvent::Done { .. } | AgentEvent::Error { .. });
                emit(event);
            }
        }
    }

    let status = child.wait().await;
    if !finished {
        let mut stderr = String::new();
        if let Some(mut err) = child.stderr.take() {
            use tokio::io::AsyncReadExt;
            let _ = err.read_to_string(&mut stderr).await;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_lookup_adds_extensions_only_on_windows() {
        let dir = std::env::temp_dir().join(format!("ag-runner-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tool.cmd"), "").unwrap();
        std::fs::write(dir.join("other"), "").unwrap();

        let here = || std::iter::once(dir.clone());
        assert_eq!(
            find_in(here(), "tool", "windows"),
            Some(dir.join("tool.cmd"))
        );
        assert_eq!(find_in(here(), "tool", "linux"), None);
        assert_eq!(find_in(here(), "other", "macos"), Some(dir.join("other")));
        let _ = std::fs::remove_dir_all(&dir);
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
            cwd: None,
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
    async fn missing_binary_reports_an_error() {
        let mut events = Vec::new();
        drive_turn("/nonexistent/agent-binary".into(), &Fake, &req(), |e| {
            events.push(e)
        })
        .await;
        assert!(matches!(events.as_slice(), [AgentEvent::Error { .. }]));
    }
}
