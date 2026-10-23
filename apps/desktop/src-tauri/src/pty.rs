//! The Terminal panel's shells: one pseudo-terminal per tab (`portable-pty`: ConPTY on Windows, a pty elsewhere).
//!
//! The UI sends keystrokes with `pty_write` and resizes with `pty_resize`; the shell's output comes back as
//! `pty-data` events carrying base64 bytes (a chunk can end in the middle of a UTF-8 character, so it is not text
//! yet), and `pty-exit` when the shell ends. Output is batched: while the UI is slow, the chunks that piled up go out
//! as one event.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, LazyLock, Mutex};

use base64::Engine;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

/// The open shells, by the tab id the UI gave them.
static SESSIONS: LazyLock<Mutex<HashMap<String, Session>>> = LazyLock::new(Default::default);

#[derive(Clone, Serialize)]
struct Data {
    id: String,
    data: String,
}

#[derive(Clone, Serialize)]
struct Exit {
    id: String,
    code: Option<u32>,
}

/// Most bytes sent in one event.
const BATCH: usize = 64 * 1024;

/// The first of `names` found in a PATH-like list of folders.
fn find_in(path: &std::ffi::OsStr, names: &[&str]) -> Option<PathBuf> {
    std::env::split_paths(path).find_map(|folder| names.iter().map(|name| folder.join(name)).find(|candidate| candidate.is_file()))
}

/// The shell to start and its arguments: PowerShell 7 when it is installed, else Windows PowerShell; `$SHELL` elsewhere.
fn shell() -> (PathBuf, Vec<&'static str>) {
    if cfg!(windows) {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let found = find_in(&path, &["pwsh.exe"]).or_else(|| find_in(&path, &["powershell.exe"]));
        return (found.unwrap_or_else(|| PathBuf::from("powershell.exe")), vec!["-NoLogo"]);
    }
    let shell = std::env::var_os("SHELL").map(PathBuf::from).filter(|path| path.is_file()).unwrap_or_else(|| PathBuf::from("/bin/sh"));
    (shell, vec!["-l"])
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from)
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows.max(1), cols: cols.max(1), pixel_width: 0, pixel_height: 0 }
}

/// Starts a shell for the tab `id` in `cwd` (the home folder when that is missing) and returns its name.
#[tauri::command]
pub fn pty_open(app: AppHandle, id: String, cwd: Option<String>, cols: u16, rows: u16) -> Result<String, String> {
    if SESSIONS.lock().map_err(|_| "terminal state is locked")?.contains_key(&id) {
        return Err(format!("terminal {id} is already open"));
    }
    let pair = native_pty_system().openpty(size(cols, rows)).map_err(|error| error.to_string())?;
    let (program, args) = shell();
    let mut command = CommandBuilder::new(&program);
    command.args(args);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    let folder = cwd.map(PathBuf::from).filter(|path| path.is_dir()).or_else(home);
    if let Some(folder) = folder.as_deref().filter(|folder| folder.is_dir()) {
        command.cwd(folder);
    }
    let child = pair.slave.spawn_command(command).map_err(|error| format!("could not start {}: {error}", program.display()))?;
    // The slave end stays open in this process only while the shell is being started.
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|error| error.to_string())?;
    let writer = pair.master.take_writer().map_err(|error| error.to_string())?;
    SESSIONS.lock().map_err(|_| "terminal state is locked")?.insert(id.clone(), Session { master: pair.master, writer, child });

    // One thread reads the shell (it blocks), one batches what it read and sends it on.
    let (sender, receiver) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 16 * 1024];
        while let Ok(count) = reader.read(&mut buffer) {
            if count == 0 || sender.send(buffer[..count].to_vec()).is_err() {
                break;
            }
        }
    });
    let tab = id.clone();
    std::thread::spawn(move || {
        let engine = base64::engine::general_purpose::STANDARD;
        while let Ok(mut bytes) = receiver.recv() {
            while bytes.len() < BATCH {
                match receiver.try_recv() {
                    Ok(more) => bytes.extend(more),
                    Err(_) => break,
                }
            }
            let _ = app.emit("pty-data", Data { id: tab.clone(), data: engine.encode(&bytes) });
        }
        // The shell closed its end: report how it ended (unless the tab was closed and the session is gone).
        let session = SESSIONS.lock().ok().and_then(|mut sessions| sessions.remove(&tab));
        if let Some(mut session) = session {
            let code = session.child.wait().ok().map(|status| status.exit_code());
            let _ = app.emit("pty-exit", Exit { id: tab, code });
        }
    });
    Ok(program.file_stem().and_then(|name| name.to_str()).unwrap_or("shell").to_string())
}

/// Keystrokes and pastes for the shell of tab `id`.
#[tauri::command]
pub fn pty_write(id: String, data: String) -> Result<(), String> {
    let mut sessions = SESSIONS.lock().map_err(|_| "terminal state is locked")?;
    let session = sessions.get_mut(&id).ok_or("that terminal is not running")?;
    session.writer.write_all(data.as_bytes()).and_then(|_| session.writer.flush()).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn pty_resize(id: String, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = SESSIONS.lock().map_err(|_| "terminal state is locked")?;
    let session = sessions.get(&id).ok_or("that terminal is not running")?;
    session.master.resize(size(cols, rows)).map_err(|error| error.to_string())
}

/// Ends the shell of tab `id` (a tab that was closed).
#[tauri::command]
pub fn pty_close(id: String) {
    let session = SESSIONS.lock().ok().and_then(|mut sessions| sessions.remove(&id));
    if let Some(mut session) = session {
        let _ = session.child.kill();
    }
}

/// The folder a new terminal opens in, for the UI to check a path before it asks for it.
#[tauri::command]
pub fn pty_default_folder() -> Option<String> {
    home().filter(|path| Path::new(path).is_dir()).map(|path| path.to_string_lossy().into_owned())
}

/// Ends every shell: they go with the app.
pub fn kill_all() {
    if let Ok(mut sessions) = SESSIONS.lock() {
        for (_, mut session) in sessions.drain() {
            let _ = session.child.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shell_is_found_and_started_with_arguments() {
        let (program, args) = shell();
        assert!(!program.as_os_str().is_empty());
        assert!(!args.is_empty());
    }

    #[test]
    fn a_batch_never_splits_a_size_below_one() {
        let size = size(0, 0);
        assert_eq!((size.cols, size.rows), (1, 1));
    }

    /// A real pseudo-terminal: runs a command through the platform shell and reads what it prints.
    #[test]
    fn a_shell_runs_a_command_and_its_output_comes_back() {
        let pair = native_pty_system().openpty(size(80, 24)).expect("a pty");
        let mut command = if cfg!(windows) {
            let mut command = CommandBuilder::new("cmd.exe");
            command.args(["/c", "echo pty-works"]);
            command
        } else {
            let mut command = CommandBuilder::new("/bin/sh");
            command.args(["-c", "echo pty-works"]);
            command
        };
        command.env("TERM", "xterm-256color");
        let mut child = pair.slave.spawn_command(command).expect("a shell");
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().expect("a reader");
        let mut writer = pair.master.take_writer().expect("a writer");
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut text = Vec::new();
            let mut buffer = [0u8; 4096];
            while let Ok(count) = reader.read(&mut buffer) {
                if count == 0 {
                    break;
                }
                text.extend_from_slice(&buffer[..count]);
                // ConPTY asks where the cursor is and waits; a terminal (xterm.js in the app) answers.
                if buffer[..count].windows(4).any(|part| part == b"[6n") {
                    let _ = writer.write_all(b"[1;1R");
                }
                if String::from_utf8_lossy(&text).contains("pty-works") {
                    break;
                }
            }
            let _ = sender.send(String::from_utf8_lossy(&text).into_owned());
        });
        let output = receiver.recv_timeout(std::time::Duration::from_secs(20)).expect("output in time");
        assert!(output.contains("pty-works"), "{output:?}");
        pair.master.resize(size(100, 30)).expect("resize");
        let _ = child.kill();
    }
}
