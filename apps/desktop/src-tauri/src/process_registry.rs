//! A file of the processes the app started, with the start time of each, so that a process left behind by an app that
//! was killed or crashed is ended at the next start. The start time is what makes this safe: a PID the system has given
//! to another program has another start time, so only a process that is still the same one is ended.
//!
//! The file is `runtime/processes.json` in the app's data folder. An entry is written when a process is adopted
//! (`process_guard::adopt_pid`) and removed when it is ended (`process_guard::kill_tree`). At start-up `reap` ends what
//! is still listed and still the same process, then empties the file.

use std::path::PathBuf;
use std::sync::{LazyLock, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub pid: u32,
    /// The start time of the process as its system reports it (a string, so each system keeps its own form).
    pub start: String,
}

/// Where the file is. Set once at start-up (`init`); until then nothing is recorded.
static FILE: OnceLock<PathBuf> = OnceLock::new();
/// Serialises the read-modify-write of the file (adopt and kill run on different threads).
static LOCK: LazyLock<Mutex<()>> = LazyLock::new(Default::default);

/// Sets the file's place: `<app data>/runtime/processes.json`.
pub fn init(app_data: PathBuf) {
    let _ = FILE.set(app_data.join("runtime").join("processes.json"));
}

/// Reads the list. A file that cannot be read as a list is moved aside (`processes.json.corrupt`) and the list starts
/// empty: a damaged file must not stop the app from starting, and its content is kept for a look.
fn load(file: &PathBuf) -> Vec<Entry> {
    let Ok(bytes) = std::fs::read(file) else { return Vec::new() };
    match serde_json::from_slice(&bytes) {
        Ok(entries) => entries,
        Err(_) => {
            let _ = std::fs::rename(file, file.with_extension("json.corrupt"));
            Vec::new()
        }
    }
}

fn save(file: &PathBuf, entries: &[Entry]) {
    if let Some(folder) = file.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Ok(text) = serde_json::to_vec_pretty(entries) {
        let _ = std::fs::write(file, text);
    }
}

/// Lists a process the app has just started. A process that cannot be read (it already ended) is not listed.
pub fn record(pid: u32) {
    let (Some(file), Some(start)) = (FILE.get(), start_time(pid)) else { return };
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut entries = load(file);
    // A process that ended by itself is never taken off by `forget` (only the app's own kills are): the list would grow
    // with every turn. Entries that are no longer the same running process go here.
    entries.retain(|entry| entry.pid != pid && start_time(entry.pid).as_deref() == Some(entry.start.as_str()));
    entries.push(Entry { pid, start });
    save(file, &entries);
}

/// Takes a process off the list once the app has ended it.
pub fn forget(pid: u32) {
    let Some(file) = FILE.get() else { return };
    let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut entries = load(file);
    let before = entries.len();
    entries.retain(|entry| entry.pid != pid);
    if entries.len() != before {
        save(file, &entries);
    }
}

/// The entries that are still the process they were: same PID, same start time.
pub fn still_same(entries: &[Entry]) -> Vec<Entry> {
    entries.iter().filter(|entry| start_time(entry.pid).as_deref() == Some(entry.start.as_str())).cloned().collect()
}

/// At start-up: ends each listed process that is still the same one (and its tree), then empties the list. A process
/// whose PID now belongs to something else is left alone.
pub fn reap(kill: impl Fn(u32)) {
    let Some(file) = FILE.get() else { return };
    // The list is read and emptied under the lock, then the kills run without it: a kill calls `forget`, which takes the
    // same lock.
    let entries = {
        let _guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let entries = load(file);
        if !entries.is_empty() {
            save(file, &[]);
        }
        entries
    };
    for entry in still_same(&entries) {
        kill(entry.pid);
    }
}

/// The start time of a process, or None when it does not exist (or cannot be read).
pub fn start_time(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        return parse_linux_start(&stat);
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ps")
            .args(["-o", "lstart=", "-p", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return (output.status.success() && !text.is_empty()).then_some(text);
    }
    #[cfg(windows)]
    {
        return windows_start(pid);
    }
    #[allow(unreachable_code)]
    None
}

/// The start time field (22nd) of a `/proc/<pid>/stat` line. The command name is in parentheses and may hold spaces, so
/// the fields are counted after its closing parenthesis (field 3 is the first one after it).
#[cfg(any(target_os = "linux", test))]
pub fn parse_linux_start(stat: &str) -> Option<String> {
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().nth(19).map(str::to_owned)
}

#[cfg(windows)]
fn windows_start(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    // SAFETY: the handle is opened for reading times and closed on every path; the FILETIME out-parameters are valid.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let (mut created, mut exit, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        let ok = GetProcessTimes(process, &mut created, &mut exit, &mut kernel, &mut user).is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return None;
        }
        let ticks = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        Some(ticks.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_linux_start_time_is_the_22nd_field_even_when_the_name_has_spaces() {
        let stat = "4242 (my (weird) app) S 1 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 987654 0 0";
        assert_eq!(parse_linux_start(stat).as_deref(), Some("987654"));
        assert_eq!(parse_linux_start("garbage"), None);
    }

    #[test]
    fn this_process_has_a_start_time_and_is_still_the_same() {
        let pid = std::process::id();
        let start = start_time(pid).expect("the test process has a start time");
        let listed = vec![Entry { pid, start: start.clone() }];
        assert_eq!(still_same(&listed), listed);
    }

    #[test]
    fn a_different_start_time_is_not_the_same_process() {
        let pid = std::process::id();
        let listed = vec![Entry { pid, start: "not-a-start-time".into() }];
        assert!(still_same(&listed).is_empty());
    }

    #[test]
    fn entries_of_processes_that_are_gone_are_dropped_when_a_new_one_is_listed() {
        let folder = std::env::temp_dir().join(format!("agentdeck-registry-test-{}", std::process::id()));
        let file = folder.join("runtime").join("processes.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let gone = Entry { pid: u32::MAX - 9, start: "1".into() };
        let alive = Entry { pid: std::process::id(), start: start_time(std::process::id()).unwrap() };
        save(&file, &[gone.clone(), alive.clone()]);
        // `record` works on the global file; the same retain rule is applied here directly.
        let mut entries = load(&file);
        entries.retain(|entry| start_time(entry.pid).as_deref() == Some(entry.start.as_str()));
        assert_eq!(entries, vec![alive]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_damaged_file_is_moved_aside_and_the_list_starts_empty() {
        let folder = std::env::temp_dir().join(format!("agentdeck-registry-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("processes.json");
        std::fs::write(&file, "{ not json").unwrap();
        assert!(load(&file).is_empty());
        assert!(!file.exists());
        assert!(folder.join("processes.json.corrupt").exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_pid_that_does_not_exist_has_no_start_time() {
        assert_eq!(start_time(u32::MAX - 7), None);
    }
}
