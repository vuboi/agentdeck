use std::path::{Path, PathBuf};

use ag_adapters::agent::claude::Claude;
use ag_adapters::agent::{AgentEvent, Provider, TurnRequest};
use ag_adapters::history::{self, HistoryMessage, HistorySession, Roots, Source};
use serde::Serialize;
use std::process::Stdio;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

fn roots() -> Result<Roots, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or("cannot find the home folder")?;
    Ok(Roots::from_home(&home))
}

/// Sessions Claude Code and Codex already saved on this machine. Read-only.
#[tauri::command]
async fn history_list() -> Result<Vec<HistorySession>, String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || history::list_all(&roots))
        .await
        .map_err(|error| error.to_string())
}

/// The transcript of one saved session. The path must be inside the history folders.
#[tauri::command]
async fn history_load(source: Source, path: String) -> Result<Vec<HistoryMessage>, String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || history::load(&roots, source, Path::new(&path)))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

/// Renames a saved session inside Claude's or Codex's own storage.
#[tauri::command]
async fn history_rename(source: Source, path: String, title: String) -> Result<(), String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || {
        history::rename(&roots, source, Path::new(&path), &title)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

/// Moves a saved session to `~/.agentdeck/trash` (not destroyed), so it leaves Claude and Codex too.
#[tauri::command]
async fn history_delete(source: Source, path: String) -> Result<(), String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || {
        history::delete(&roots, source, Path::new(&path)).map(|_| ())
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

/// Text of one file for the file viewer. Read-only, text only, at most 2 MB, and the file must sit
/// inside `root` (the project folder), so a path from the UI cannot reach outside it.
#[tauri::command]
async fn file_read(root: String, path: String) -> Result<String, String> {
    const MAX_BYTES: u64 = 2 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("file is outside the project folder".to_string());
        }
        let size = std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len();
        if size > MAX_BYTES {
            return Err(format!("file is {size} bytes, the viewer stops at 2 MB"));
        }
        std::fs::read_to_string(&path).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Bytes of one file for previews (images, PDF, SVG), base64-encoded. Same rules as `file_read`:
/// read-only, inside `root`, and at most 20 MB.
#[tauri::command]
async fn file_read_bytes(root: String, path: String) -> Result<String, String> {
    use base64::Engine;
    const MAX_BYTES: u64 = 20 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("file is outside the project folder".to_string());
        }
        let size = std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len();
        if size > MAX_BYTES {
            return Err(format!("file is {size} bytes, the preview stops at 20 MB"));
        }
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// One event of one chat, sent to the UI on the `agent-event` channel.
#[derive(Clone, Serialize)]
struct AgentEnvelope {
    session: String,
    event: AgentEvent,
}

fn provider_for(agent: &str) -> Result<Box<dyn Provider>, String> {
    match agent {
        "claude" => Ok(Box::new(Claude)),
        other => Err(format!("no agent provider for `{other}` yet")),
    }
}

/// The binary to run: the provider's own candidate paths first, then whatever PATH resolves.
fn resolve_binary(provider: &dyn Provider) -> std::ffi::OsString {
    provider
        .candidates()
        .into_iter()
        .find(|path| path.is_file())
        .map(|path| path.into_os_string())
        .unwrap_or_else(|| provider.binary().into())
}

/// Runs one turn: start the CLI, read its stream line by line, translate each line, hand each event to `emit`.
/// Separate from Tauri so the process handling can be tested with a stand-in binary.
async fn drive_turn(
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

/// Runs one turn for the app: resolves the binary, then forwards every event to the UI.
async fn run_turn(app: AppHandle, session: String, provider: Box<dyn Provider>, req: TurnRequest) {
    let binary = resolve_binary(provider.as_ref());
    drive_turn(binary, provider.as_ref(), &req, |event| {
        let _ = app.emit(
            "agent-event",
            AgentEnvelope {
                session: session.clone(),
                event,
            },
        );
    })
    .await;
}

#[cfg(all(test, unix))] // the fake agents are sh scripts
mod agent_turn_tests {
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
            ag_adapters::agent::claude::Claude.parse_line(line)
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

/// Sends one user message to an agent CLI. Returns at once; the answer arrives as `agent-event`s.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // one parameter per field the frontend sends
async fn agent_send(
    app: AppHandle,
    session: String,
    agent: String,
    prompt: String,
    agent_session: Option<String>,
    resume: bool,
    model: Option<String>,
    cwd: Option<String>,
) -> Result<(), String> {
    let provider = provider_for(&agent)?;
    let req = TurnRequest {
        prompt,
        session_id: agent_session,
        resume,
        model,
        cwd: cwd.map(Into::into),
    };
    tauri::async_runtime::spawn(run_turn(app, session, provider, req));
    Ok(())
}

/// Starts the AgentDeck desktop window.
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            history_list,
            history_load,
            history_rename,
            history_delete,
            file_read,
            file_read_bytes,
            agent_send
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AgentDeck desktop application");
}
