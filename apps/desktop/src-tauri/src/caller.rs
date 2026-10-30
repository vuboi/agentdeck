//! Who is calling the MCP bridge, when the URL does not say.
//!
//! Most agent CLIs get a per-chat URL (`/mcp?chat=<id>`), so the bridge knows the caller. Antigravity reads one shared
//! `~/.gemini/config/mcp_config.json` (agy has no per-run MCP config: `--settings` refuses `mcpServers`, and moving its
//! config dir means moving its home), so its calls arrive without a chat. They are identified here, in this order:
//!
//! 1. `Mcp-Session-Id`: once a connection was identified, the session the bridge handed out remembers the chat.
//! 2. The connection: the peer port of the TCP stream -> the process that owns it -> its ancestors, until one is a CLI
//!    this app started for a chat (`ag_adapters::agent::runner::chat_of_pid`).
//!
//! Anything else stays unknown (`""`); the UI refuses to delegate without an identified caller.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Session id handed out by the bridge -> the chat it was identified as.
static SESSIONS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

fn sessions() -> std::sync::MutexGuard<'static, HashMap<String, String>> {
    SESSIONS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The chat a session id was identified as.
pub fn chat_of_session(session: &str) -> Option<String> {
    sessions().get(session).cloned()
}

/// Remembers that session (an id the client sent back) belongs to chat.
pub fn remember_session(session: &str, chat: &str) {
    sessions().insert(session.to_owned(), chat.to_owned());
}

/// A new session id for a client identified as `chat` (empty: not identified, the session still carries no chat).
pub fn open_session(chat: &str) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let mut map = sessions();
    if map.len() > 512 {
        map.clear();
    }
    if !chat.is_empty() {
        map.insert(id.clone(), chat.to_owned());
    }
    id
}

/// The value of a header in a request head (case-insensitive name).
pub fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// The chat behind a connection to the bridge: the CLI process of a chat that is, or owns, the other end.
/// `local_port` is the port the bridge listens on, `peer_port` the client's.
pub fn chat_of_connection(peer_port: u16, local_port: u16) -> Option<String> {
    let owner = owner_of_connection(peer_port, local_port)?;
    let parents = process_parents();
    let mut pid = owner;
    // A bounded walk: a parent map from a snapshot can hold a reused pid, so it can loop.
    for _ in 0..32 {
        if let Some(chat) = ag_adapters::agent::runner::chat_of_pid(pid) {
            return Some(chat);
        }
        match parents.get(&pid) {
            Some(&parent) if parent != 0 && parent != pid => pid = parent,
            _ => break,
        }
    }
    None
}

#[cfg(windows)]
fn owner_of_connection(peer_port: u16, local_port: u16) -> Option<u32> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
    };
    use windows::Win32::Networking::WinSock::AF_INET;
    let mut size = 0u32;
    // SAFETY: the first call only asks for the size; the second fills a buffer of that size.
    unsafe {
        GetExtendedTcpTable(None, &mut size, false, AF_INET.0 as u32, TCP_TABLE_OWNER_PID_ALL, 0);
    }
    // Rows come and go between the two calls: a few tries with room to spare.
    for _ in 0..4 {
        // A u32 buffer provides the alignment required by MIB_TCPTABLE_OWNER_PID.
        let mut buffer = vec![0u32; (size as usize + 4096).div_ceil(4)];
        let mut length = (buffer.len() * 4) as u32;
        // SAFETY: `buffer` is `length` bytes long.
        let status = unsafe {
            GetExtendedTcpTable(
                Some(buffer.as_mut_ptr().cast()),
                &mut length,
                false,
                AF_INET.0 as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if status != 0 {
            size = length;
            continue;
        }
        // SAFETY: on success the buffer starts with a MIB_TCPTABLE_OWNER_PID of `dwNumEntries` rows.
        let table = unsafe { &*(buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
        let rows = unsafe {
            std::slice::from_raw_parts(table.table.as_ptr(), table.dwNumEntries as usize)
        };
        // The client's end: it listens on nothing, its local port is the peer port and it talks to the bridge port.
        return rows
            .iter()
            .find(|row| {
                u16::from_be(row.dwLocalPort as u16) == peer_port && u16::from_be(row.dwRemotePort as u16) == local_port
            })
            .map(|row| row.dwOwningPid);
    }
    None
}

#[cfg(windows)]
fn process_parents() -> HashMap<u32, u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    let mut parents = HashMap::new();
    // SAFETY: a snapshot handle is closed below; the entry is sized as the API requires.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return parents;
        };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    parents
}

#[cfg(not(windows))]
fn owner_of_connection(peer_port: u16, local_port: u16) -> Option<u32> {
    // `lsof` lists the sockets of the client's end: the one whose local port is the peer port.
    let output = std::process::Command::new("lsof")
        .args(["-nP", "-Fpn", &format!("-iTCP@127.0.0.1:{local_port}"), "-sTCP:ESTABLISHED"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let (mut pid, mut found) = (0u32, None);
    for line in text.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse().unwrap_or(0);
        } else if let Some(name) = line.strip_prefix('n') {
            // `127.0.0.1:<client port>-><127.0.0.1:bridge port>`
            if name.starts_with(&format!("127.0.0.1:{peer_port}->")) {
                found = Some(pid);
            }
        }
    }
    found.filter(|pid| *pid != 0)
}

#[cfg(not(windows))]
fn process_parents() -> HashMap<u32, u32> {
    let Ok(output) = std::process::Command::new("ps").args(["-A", "-o", "pid=,ppid="]).output() else {
        return HashMap::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_remembers_its_chat() {
        let id = open_session("chat-1");
        assert_eq!(chat_of_session(&id).as_deref(), Some("chat-1"));
        let anonymous = open_session("");
        assert_eq!(chat_of_session(&anonymous), None);
        assert_eq!(chat_of_session("nope"), None);
    }

    #[test]
    fn headers_are_found_by_name_without_case() {
        let head = "POST /mcp HTTP/1.1\r\nHost: x\r\nmcp-session-id:  abc \r\n";
        assert_eq!(header(head, "Mcp-Session-Id"), Some("abc"));
        assert_eq!(header(head, "Origin"), None);
    }

    #[test]
    fn a_connection_of_this_process_is_walked_up_to_its_chat() {
        // A real loopback connection: both ends are this process, which stands for the CLI of a chat.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let bridge = listener.local_addr().unwrap().port();
        let client = std::net::TcpStream::connect(("127.0.0.1", bridge)).unwrap();
        let (_server_side, peer) = listener.accept().unwrap();
        assert_eq!(peer.port(), client.local_addr().unwrap().port());
        assert_eq!(chat_of_connection(peer.port(), bridge), None);
        ag_adapters::agent::runner::claim_pid(std::process::id(), "chat-7");
        assert_eq!(chat_of_connection(peer.port(), bridge).as_deref(), Some("chat-7"));
        ag_adapters::agent::runner::release_pid(std::process::id());
    }
}

/// Samples CPU and I/O of live descendants of the CLI started for this chat. An error is not an idle result.
#[tauri::command]
pub async fn worker_descendant_activity(chat: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || descendant_activity(&chat))
        .await.map_err(|error| error.to_string())?
}

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct ProcessSample {
    created: u64,
    cpu: u64,
    io: u64,
}

#[cfg(windows)]
fn descendant_activity(chat: &str) -> Result<bool, String> {
    use std::collections::{HashMap, HashSet};
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows::Win32::System::Threading::{GetProcessIoCounters, GetProcessTimes, OpenProcess, IO_COUNTERS, PROCESS_QUERY_LIMITED_INFORMATION};

    type Samples = HashMap<u32, ProcessSample>;
    static PREVIOUS: LazyLock<Mutex<HashMap<String, (Instant, Samples)>>> = LazyLock::new(Default::default);
    let roots = ag_adapters::agent::runner::pids_of_chat(chat);
    let mut previous = PREVIOUS.lock().unwrap_or_else(|p| p.into_inner());
    previous.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(300));
    if roots.is_empty() {
        previous.remove(chat);
        return Ok(false);
    }
    let mut parents = HashMap::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).map_err(|e| e.to_string())?;
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if let Err(error) = Process32FirstW(snapshot, &mut entry) {
            let _ = CloseHandle(snapshot);
            return Err(error.to_string());
        }
        loop {
            parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
            if Process32NextW(snapshot, &mut entry).is_err() { break; }
        }
        let _ = CloseHandle(snapshot);
    }
    let roots: HashSet<u32> = roots.into_iter().collect();
    let mut samples = HashMap::new();
    for &pid in parents.keys() {
        if roots.contains(&pid) { continue; }
        let mut ancestor = pid;
        let mut descendant = false;
        for _ in 0..128 {
            let Some(&parent) = parents.get(&ancestor) else { break };
            if roots.contains(&parent) { descendant = true; break; }
            if parent == 0 || parent == ancestor { break; }
            ancestor = parent;
        }
        if !descendant { continue; }
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(|e| e.to_string())?;
            let (mut creation, mut exit, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
            let mut io = IO_COUNTERS::default();
            let times = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
            let counters = GetProcessIoCounters(handle, &mut io);
            let _ = CloseHandle(handle);
            times.map_err(|e| e.to_string())?;
            counters.map_err(|e| e.to_string())?;
            let value = |t: FILETIME| ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64;
            samples.insert(pid, ProcessSample {
                created: value(creation), cpu: value(kernel).saturating_add(value(user)),
                io: io.ReadOperationCount.saturating_add(io.WriteOperationCount).saturating_add(io.OtherOperationCount),
            });
        }
    }
    let active = samples.iter().any(|(pid, sample)| {
        previous.get(chat).and_then(|(_, old)| old.get(pid)).is_none_or(|old| {
            old.created != sample.created || sample.cpu > old.cpu || sample.io > old.io
        })
    });
    previous.insert(chat.to_owned(), (Instant::now(), samples));
    Ok(active)
}

#[cfg(not(windows))]
fn descendant_activity(_chat: &str) -> Result<bool, String> {
    Ok(false)
}
