//! Runs one agent turn: finds the CLI, starts it, reads its output line by line and turns each line into events.
//! Kept apart from the app so the process handling is tested with stand-in binaries and runs the same on
//! Windows, macOS and Linux.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
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

/// Install folders shared by the three CLIs (npm, pnpm, bun, volta, scoop, brew, version managers), beyond the exact
/// paths each provider names. A desktop app started from the Dock, the Start menu or a launcher gets a short PATH.
fn common_dirs(
    os: &str,
    home: Option<&Path>,
    local: Option<&Path>,
    appdata: Option<&Path>,
) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let under = |base: Option<&Path>, parts: &[&str]| {
        base.map(|base| {
            parts
                .iter()
                .fold(base.to_path_buf(), |path, part| path.join(part))
        })
    };
    let mut push = |path: Option<PathBuf>| dirs.extend(path);
    // Node version managers keep one `bin` folder per installed Node: newest first.
    let versions = |parent: Option<PathBuf>, tail: &[&str]| -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = parent
            .and_then(|parent| std::fs::read_dir(parent).ok())
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| tail.iter().fold(entry.path(), |path, part| path.join(part)))
            .collect();
        found.sort();
        found.reverse();
        found
    };
    match os {
        "windows" => {
            push(under(appdata, &["npm"]));
            push(under(local, &["pnpm"]));
            push(std::env::var_os("PNPM_HOME").map(PathBuf::from));
            push(under(local, &["Volta", "bin"]));
            push(under(local, &["Microsoft", "WinGet", "Links"]));
            push(under(home, &[".volta", "bin"]));
            push(under(home, &[".bun", "bin"]));
            push(under(home, &["scoop", "shims"]));
            push(under(home, &[".cargo", "bin"]));
            push(under(home, &[".local", "bin"]));
            for dir in versions(under(appdata, &["nvm"]), &[]) {
                push(Some(dir));
            }
            // mise installs Node itself under LOCALAPPDATA, and npm -g puts command shims
            // beside that Node executable. A GUI-launched Tauri process does not inherit mise's
            // shell shim PATH, so include every installed Node version here as well.
            for dir in versions(under(local, &["mise", "installs", "node"]), &[]) {
                push(Some(dir));
            }
        }
        _ => {
            push(under(home, &[".local", "bin"]));
            if os == "macos" {
                push(Some(PathBuf::from("/opt/homebrew/bin")));
                push(under(home, &["Library", "pnpm"]));
            } else {
                push(Some(PathBuf::from("/home/linuxbrew/.linuxbrew/bin")));
                push(Some(PathBuf::from("/snap/bin")));
                push(under(home, &[".local", "share", "pnpm"]));
            }
            push(Some(PathBuf::from("/usr/local/bin")));
            push(Some(PathBuf::from("/usr/bin")));
            push(under(home, &[".npm-global", "bin"]));
            push(under(home, &[".volta", "bin"]));
            push(under(home, &[".bun", "bin"]));
            push(under(home, &[".cargo", "bin"]));
            push(under(home, &[".asdf", "shims"]));
            push(under(home, &[".local", "share", "mise", "shims"]));
            for dir in versions(under(home, &[".nvm", "versions", "node"]), &["bin"]) {
                push(Some(dir));
            }
            let fnm = if os == "macos" {
                under(
                    home,
                    &["Library", "Application Support", "fnm", "node-versions"],
                )
            } else {
                under(home, &[".local", "share", "fnm", "node-versions"])
            };
            for dir in versions(fnm, &["installation", "bin"]) {
                push(Some(dir));
            }
        }
    }
    dirs
}

/// Adds the same well-known CLI folders used for resolution to the child process PATH. This matters
/// for npm shims such as `opencode.cmd`: the shim may need its matching `node` executable on PATH.
fn augmented_path() -> Option<OsString> {
    let mut dirs = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from);
    dirs.extend(common_dirs(
        std::env::consts::OS,
        home.as_deref(),
        local.as_deref(),
        appdata.as_deref(),
    ));
    std::env::join_paths(dirs).ok()
}

/// The PATH a login shell gives: what the user's terminal sees, which a GUI app on macOS and Linux does not inherit
/// (it is where npm, nvm, brew and the like are put on PATH). Asked once; a slow or broken shell is given up on.
#[cfg(unix)]
fn login_shell_path() -> Option<OsString> {
    use std::sync::OnceLock;
    static PATH: OnceLock<Option<OsString>> = OnceLock::new();
    PATH.get_or_init(|| {
        let shell = std::env::var_os("SHELL")
            .filter(|shell| !shell.is_empty())
            .unwrap_or_else(|| "/bin/sh".into());
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let output = std::process::Command::new(shell)
                .args(["-ilc", "printf '__AGPATH__%s__AGPATH__' \"$PATH\""])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output();
            let _ = send.send(output);
        });
        let output = receive
            .recv_timeout(std::time::Duration::from_secs(4))
            .ok()?
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let path = text.split("__AGPATH__").nth(1)?.trim();
        (!path.is_empty()).then(|| OsString::from(path))
    })
    .clone()
}

#[cfg(not(unix))]
fn login_shell_path() -> Option<OsString> {
    None
}

/// The binary to run, first match wins: `AGENTDECK_<NAME>_BIN` (a path the user pinned), the provider's own known
/// locations, PATH, the install folders every CLI shares, the PATH of a login shell, and last the bare name so
/// the OS can report "not found".
pub fn resolve_binary(provider: &dyn Provider) -> OsString {
    let pinned = std::env::var_os(format!("AGENTDECK_{}_BIN", provider.id().to_uppercase()))
        .map(PathBuf::from)
        .filter(|path| path.is_file());
    let os = std::env::consts::OS;
    let from_path = || {
        let path = std::env::var_os("PATH")?;
        find_in(std::env::split_paths(&path), provider.binary(), os)
    };
    let from_common = || {
        let env_path = |key: &str| std::env::var_os(key).map(PathBuf::from);
        let home = env_path("HOME").or_else(|| env_path("USERPROFILE"));
        let dirs = common_dirs(
            os,
            home.as_deref(),
            env_path("LOCALAPPDATA").as_deref(),
            env_path("APPDATA").as_deref(),
        );
        find_in(dirs.into_iter(), provider.binary(), os)
    };
    let from_shell = || {
        find_in(
            std::env::split_paths(&login_shell_path()?),
            provider.binary(),
            os,
        )
    };
    pinned
        .or_else(|| {
            provider
                .candidates()
                .into_iter()
                .find(|path| path.is_file())
        })
        .or_else(from_path)
        .or_else(from_common)
        .or_else(from_shell)
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

    let mut finished = false;
    if let Some(stdout) = child.stdout.take() {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            for event in provider.parse_line(&line) {
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

    let status = if finished {
        let _ = child.start_kill();
        child.wait().await
    } else {
        child.wait().await
    };
    if !finished {
        if let Some(event) = provider.finish(req) {
            emit(event);
            return;
        }
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

    #[test]
    fn common_dirs_differ_per_platform_and_cover_the_usual_installs() {
        let home = Path::new("/h");
        let mac = common_dirs("macos", Some(home), None, None);
        assert!(mac.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert!(mac.contains(&home.join(".local").join("bin")));
        assert!(!mac.contains(&PathBuf::from("/snap/bin")));
        let linux = common_dirs("linux", Some(home), None, None);
        assert!(linux.contains(&PathBuf::from("/snap/bin")));
        assert!(linux.contains(&home.join(".npm-global").join("bin")));
        let (local, appdata) = (Path::new("/l"), Path::new("/a"));
        let windows = common_dirs("windows", Some(home), Some(local), Some(appdata));
        assert!(windows.contains(&appdata.join("npm")));
        assert!(windows.contains(&local.join("pnpm")));
        assert!(windows.contains(&home.join("scoop").join("shims")));
        assert!(!windows.contains(&PathBuf::from("/usr/bin")));
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
    async fn missing_binary_reports_an_error() {
        let mut events = Vec::new();
        drive_turn("/nonexistent/agent-binary".into(), &Fake, &req(), |e| {
            events.push(e)
        })
        .await;
        assert!(matches!(events.as_slice(), [AgentEvent::Error { .. }]));
    }
}
