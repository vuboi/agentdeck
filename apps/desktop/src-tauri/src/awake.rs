//! Keeps the computer from going to sleep while AgentDeck runs: an agent working on a long task, or a reply being
//! waited for, is cut off when the system suspends. Display sleep is not prevented (the screen may still turn off).
//!
//! - Windows: `SetThreadExecutionState` with `ES_CONTINUOUS | ES_SYSTEM_REQUIRED`, held by a thread of its own (the
//!   state lasts as long as the thread that set it).
//! - macOS: `caffeinate -i -s -w <our pid>`: ends by itself when the app does.
//! - Linux: `systemd-inhibit` around a loop that ends when the app does (a lock that holds until the app exits).
//!
//! Best effort: a system without the tool simply keeps its own sleep rules, and the command says so.

use std::sync::Mutex;

/// What holds the system awake now.
enum Hold {
    #[cfg(windows)]
    Thread(std::sync::mpsc::Sender<()>),
    #[cfg(not(windows))]
    Process(std::process::Child),
}

static HOLD: Mutex<Option<Hold>> = Mutex::new(None);

#[cfg(windows)]
fn acquire() -> Result<Hold, String> {
    use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
    let (stop, wait) = std::sync::mpsc::channel::<()>();
    let (ready, ok) = std::sync::mpsc::channel::<bool>();
    std::thread::spawn(move || {
        // SAFETY: a plain Win32 call; the state belongs to this thread until it ends or clears it.
        let before = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
        let _ = ready.send(before.0 != 0);
        let _ = wait.recv(); // until released (or the sender is dropped with the app)
        // SAFETY: as above.
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    });
    match ok.recv() {
        Ok(true) => Ok(Hold::Thread(stop)),
        _ => Err("Windows did not accept the request".to_string()),
    }
}

#[cfg(target_os = "macos")]
fn acquire() -> Result<Hold, String> {
    std::process::Command::new("caffeinate")
        .args(["-i", "-s", "-w", &std::process::id().to_string()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(Hold::Process)
        .map_err(|error| format!("could not run caffeinate: {error}"))
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn acquire() -> Result<Hold, String> {
    let wait = format!("while kill -0 {} 2>/dev/null; do sleep 5; done", std::process::id());
    std::process::Command::new("systemd-inhibit")
        .args(["--what=idle:sleep", "--who=AgentDeck", "--why=An agent is working", "--mode=block", "sh", "-c", &wait])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(Hold::Process)
        .map_err(|error| format!("could not run systemd-inhibit: {error}"))
}

/// Starts (`on`) or stops holding the system awake. Asking for what is already so changes nothing.
#[tauri::command]
pub fn keep_awake_set(on: bool) -> Result<(), String> {
    let mut hold = HOLD.lock().map_err(|_| "busy".to_string())?;
    if on {
        if hold.is_none() {
            *hold = Some(acquire()?);
        }
        return Ok(());
    }
    match hold.take() {
        #[cfg(windows)]
        Some(Hold::Thread(stop)) => {
            let _ = stop.send(());
        }
        #[cfg(not(windows))]
        Some(Hold::Process(mut child)) => {
            let _ = child.kill();
            let _ = child.wait();
        }
        None => {}
    }
    Ok(())
}

/// Lets go when the app ends (the helper processes would end with it anyway).
pub fn release() {
    let _ = keep_awake_set(false);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asking twice changes nothing, and letting go twice is harmless.
    #[test]
    fn holding_and_letting_go_can_be_repeated() {
        if keep_awake_set(true).is_err() {
            return; // a machine without the tool (a bare Linux container) keeps its own rules
        }
        assert!(keep_awake_set(true).is_ok());
        assert!(keep_awake_set(false).is_ok());
        assert!(keep_awake_set(false).is_ok());
    }
}
