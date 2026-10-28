// What the chat does with a file path in an answer (path-links.ts finds it): checks that it exists, opens it, shows it
// in the file manager, and fetches a picture from the web for the answer to show. The commands to launch are built by
// plain functions per OS, so the tests can check macOS and Linux on any machine.
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

pub const THIS_OS: Os = if cfg!(windows) {
    Os::Windows
} else if cfg!(target_os = "macos") {
    Os::Mac
} else {
    Os::Linux
};

/// A program to start. `raw` is one argument written as it must be on the command line (Explorer's `/select,"…"` breaks
/// when it is quoted the usual way).
#[derive(Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: &'static str,
    pub args: Vec<String>,
    pub raw: Option<String>,
}

/// Opens a file with the program the system has for it (a folder opens in the file manager).
pub fn open_launch(os: Os, path: &str) -> Launch {
    match os {
        // `start` is a cmd built-in; the empty title keeps a quoted path from being taken for it.
        Os::Windows => Launch { program: "cmd", args: vec!["/C".into(), "start".into(), String::new(), path.into()], raw: None },
        Os::Mac => Launch { program: "open", args: vec![path.into()], raw: None },
        Os::Linux => Launch { program: "xdg-open", args: vec![path.into()], raw: None },
    }
}

/// Shows a file in the file manager: selected on Windows and macOS; on Linux there is no common way to select, so the
/// folder that holds it is opened.
pub fn reveal_launch(os: Os, path: &str) -> Launch {
    match os {
        Os::Windows => Launch { program: "explorer", args: vec![], raw: Some(format!("/select,\"{}\"", path.replace('/', "\\"))) },
        Os::Mac => Launch { program: "open", args: vec!["-R".into(), path.into()], raw: None },
        Os::Linux => {
            let folder = Path::new(path).parent().map(|parent| parent.to_string_lossy().into_owned()).filter(|parent| !parent.is_empty());
            Launch { program: "xdg-open", args: vec![folder.unwrap_or_else(|| "/".into())], raw: None }
        }
    }
}

fn start(launch: Launch) -> Result<(), String> {
    let mut command = std::process::Command::new(launch.program);
    command.args(&launch.args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window for `cmd /C start`.
        command.creation_flags(0x0800_0000);
        if let Some(raw) = &launch.raw {
            command.raw_arg(raw);
        }
    }
    #[cfg(not(windows))]
    let _ = &launch.raw;
    command.spawn().map(|_| ()).map_err(|error| error.to_string())
}

fn home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// The ways one path may be spelled on disk: `~/` is the home folder, and on macOS a name may be stored composed (NFC)
/// or decomposed (NFD), so both are tried. A trailing slash is never added or removed.
fn spellings(path: &str) -> Vec<PathBuf> {
    let expanded = match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => home().map(|home| home.join(rest)).unwrap_or_else(|| PathBuf::from(path)),
        None => PathBuf::from(path),
    };
    let mut out = vec![expanded.clone()];
    let text = expanded.to_string_lossy();
    for other in [text.nfc().collect::<String>(), text.nfd().collect::<String>()] {
        if other != text {
            out.push(PathBuf::from(other));
        }
    }
    out
}

/// The file or folder a path names, as it is spelled on this machine, or None.
pub fn find(path: &str) -> Option<PathBuf> {
    spellings(path).into_iter().find(|candidate| candidate.exists())
}

/// A path without the `\\?\` prefix Windows adds to resolved ones (Explorer does not take it with `/select`).
fn plain(path: &Path) -> String {
    let text = path.to_string_lossy().into_owned();
    text.strip_prefix("\\\\?\\UNC\\").map(|rest| format!("\\\\{rest}")).or_else(|| text.strip_prefix("\\\\?\\").map(str::to_string)).unwrap_or(text)
}

#[derive(Serialize)]
pub struct Resolved {
    pub exists: bool,
    pub dir: bool,
    /// The path as it is on disk here (home expanded, the spelling that exists): what to open.
    pub path: String,
}

/// For each guess: does it exist here. The caller keeps the longest that does.
#[tauri::command]
pub async fn path_resolve(paths: Vec<String>) -> Vec<Resolved> {
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|path| match find(&path) {
                Some(found) => Resolved { exists: true, dir: found.is_dir(), path: plain(&found) },
                None => Resolved { exists: false, dir: false, path },
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// Opens a file (or folder) the way a double click does. A link to a file is followed by the system.
#[tauri::command]
pub fn path_open(path: String) -> Result<(), String> {
    let found = find(&path).ok_or_else(|| format!("{path} not found"))?;
    start(open_launch(THIS_OS, &plain(&found)))
}

/// Shows a file in the file manager, selected where the system can. A link is resolved first: the folder shown is the
/// real one, not the folder the link sits in.
#[tauri::command]
pub fn path_reveal(path: String) -> Result<(), String> {
    let found = find(&path).ok_or_else(|| format!("{path} not found"))?;
    let real = std::fs::canonicalize(&found).unwrap_or(found);
    start(reveal_launch(THIS_OS, &plain(&real)))
}

const IMAGE_MAX_BYTES: usize = 15 * 1024 * 1024;

/// An address the app must not fetch for a page: the local network. Loopback stays allowed (a picture served by the
/// agent on localhost is the point of the house style).
pub fn blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local() || ip.is_unspecified() || ip.is_broadcast() || ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]),
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return blocked_ip(IpAddr::V4(v4));
            }
            ip.is_unspecified() || (ip.segments()[0] & 0xfe00) == 0xfc00 || (ip.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// The type of a picture from its first bytes (PNG, JPEG, GIF, WebP, BMP, ICO).
fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        [b'B', b'M', ..] => Some("image/bmp"),
        [0, 0, 1, 0, ..] => Some("image/x-icon"),
        _ => None,
    }
}

fn cache_file(url: &str) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(url.as_bytes());
    let name: String = digest.iter().take(16).map(|byte| format!("{byte:02x}")).collect();
    std::env::temp_dir().join("agentdeck-images").join(name)
}

/// A picture from the web as a `data:` URL, kept in a cache folder (the window may not load it itself). Only http(s),
/// only images, at most 15 MB, never an address on the local network.
#[tauri::command]
pub async fn image_fetch(url: String) -> Result<String, String> {
    use base64::Engine;
    let parsed = reqwest::Url::parse(&url).map_err(|error| error.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("only http and https".into());
    }
    let host = parsed.host_str().ok_or("no host")?.to_string();
    let port = parsed.port_or_known_default().unwrap_or(80);
    if host != "localhost" {
        let addresses: Vec<IpAddr> = match host.parse::<IpAddr>() {
            Ok(ip) => vec![ip],
            Err(_) => tokio::net::lookup_host((host.as_str(), port)).await.map_err(|error| error.to_string())?.map(|address| address.ip()).collect(),
        };
        if addresses.iter().any(|ip| blocked_ip(*ip)) {
            return Err("address on the local network".into());
        }
    }
    let file = cache_file(&url);
    let meta = file.with_extension("type");
    if let (Ok(bytes), Ok(kind)) = (std::fs::read(&file), std::fs::read_to_string(&meta)) {
        return Ok(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)));
    }
    // Sites such as Wikimedia refuse a request with no User-Agent (and answer 429 to a bare client).
    let client = reqwest::Client::builder()
        .user_agent(concat!("Mozilla/5.0 AgentDeck/", env!("CARGO_PKG_VERSION"), " (image preview)"))
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(4))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client.get(parsed).send().await.map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("status {}", response.status()));
    }
    let kind = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.split(';').next().unwrap_or("").trim().to_ascii_lowercase())
        .unwrap_or_default();
    if response.content_length().is_some_and(|length| length > IMAGE_MAX_BYTES as u64) {
        return Err("larger than 15 MB".into());
    }
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    if bytes.len() > IMAGE_MAX_BYTES {
        return Err("larger than 15 MB".into());
    }
    // A server may label a picture `application/octet-stream`: the bytes decide.
    let kind = if kind.starts_with("image/") { kind } else { sniff_image(&bytes).ok_or("not an image")?.to_string() };
    if let Some(folder) = file.parent() {
        let _ = std::fs::create_dir_all(folder);
        let _ = std::fs::write(&file, &bytes);
        let _ = std::fs::write(&meta, &kind);
    }
    Ok(format!("data:{kind};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_opens_with_start_and_selects_with_explorer() {
        let open = open_launch(Os::Windows, "C:\\Users\\me\\My Docs\\a.txt");
        assert_eq!(open.program, "cmd");
        assert_eq!(open.args, ["/C", "start", "", "C:\\Users\\me\\My Docs\\a.txt"]);
        let reveal = reveal_launch(Os::Windows, "C:/Users/me/My Docs/a.txt");
        assert_eq!(reveal.program, "explorer");
        assert_eq!(reveal.raw.as_deref(), Some("/select,\"C:\\Users\\me\\My Docs\\a.txt\""));
    }

    #[test]
    fn mac_opens_with_open_and_reveals_with_dash_r() {
        assert_eq!(open_launch(Os::Mac, "/Users/me/a b.txt"), Launch { program: "open", args: vec!["/Users/me/a b.txt".into()], raw: None });
        assert_eq!(reveal_launch(Os::Mac, "/Users/me/a b.txt").args, ["-R", "/Users/me/a b.txt"]);
    }

    #[test]
    fn linux_opens_with_xdg_open_and_reveals_the_parent_folder() {
        assert_eq!(open_launch(Os::Linux, "/home/me/a.txt").program, "xdg-open");
        let reveal = reveal_launch(Os::Linux, "/home/me/project/a.txt");
        assert_eq!((reveal.program, reveal.args.as_slice()), ("xdg-open", ["/home/me/project".to_string()].as_slice()));
        assert_eq!(reveal_launch(Os::Linux, "a.txt").args, ["/"]);
    }

    #[test]
    fn a_picture_is_known_by_its_first_bytes() {
        assert_eq!(sniff_image(&[0x89, b'P', b'N', b'G', 0, 0]), Some("image/png"));
        assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_image(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_image(b"<html>"), None);
    }

    #[test]
    fn a_path_that_is_not_there_is_not_found() {
        assert!(find("/definitely/not/here/x.txt").is_none());
        assert!(find("C:\\definitely\\not\\here\\x.txt").is_none());
    }

    #[test]
    fn an_existing_path_is_found_and_a_trailing_slash_is_left_alone() {
        let folder = std::env::temp_dir();
        let text = folder.to_string_lossy().into_owned();
        assert!(find(&text).is_some());
        assert!(find(&format!("{text}{}", std::path::MAIN_SEPARATOR)).is_some());
    }

    #[test]
    fn composed_and_decomposed_names_find_the_same_file() {
        let folder = std::env::temp_dir().join(format!("agentdeck-nfc-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let composed = "b\u{00e1}o c\u{00e1}o.txt";
        std::fs::write(folder.join(composed), "x").unwrap();
        let decomposed: String = composed.nfd().collect();
        assert!(find(&folder.join(composed).to_string_lossy()).is_some());
        // On a file system that keeps the bytes as written (Windows, Linux) the other spelling is found by its own try.
        assert!(find(&folder.join(&decomposed).to_string_lossy()).is_some());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_local_network_is_blocked_but_loopback_and_the_web_are_not() {
        for ip in ["10.0.0.5", "192.168.1.1", "172.16.0.9", "169.254.1.1", "0.0.0.0", "100.64.0.1", "fc00::1", "fe80::1", "::ffff:10.0.0.1"] {
            assert!(blocked_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["127.0.0.1", "::1", "8.8.8.8", "1.1.1.1", "2606:4700::1111"] {
            assert!(!blocked_ip(ip.parse().unwrap()), "{ip}");
        }
    }
}
