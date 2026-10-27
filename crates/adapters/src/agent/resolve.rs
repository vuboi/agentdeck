//! Finds an agent CLI's binary: a pinned path, the provider's known locations, PATH, the install folders every CLI
//! shares, and the PATH of a login shell. A desktop app started from the Dock, the Start menu or a launcher gets a
//! short PATH, so looking only at PATH would miss most installs.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::Provider;

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
pub(super) fn augmented_path() -> Option<OsString> {
    let current = std::env::var_os("PATH");
    let mut cache = PATH_CACHE.lock().ok()?;
    if let Some((key, at, value)) = cache.as_ref() {
        if *key == current && at.elapsed() < CACHE_TTL {
            return value.clone();
        }
    }
    let value = build_augmented_path();
    *cache = Some((current, Instant::now(), value.clone()));
    value
}

/// How long a computed PATH or binary location is reused. The folders it was built from change only when the user
/// installs a CLI or a Node version, so a minute of staleness costs nothing; a changed PATH invalidates at once.
const CACHE_TTL: Duration = Duration::from_secs(60);

type PathCache = Mutex<Option<(Option<OsString>, Instant, Option<OsString>)>>;
static PATH_CACHE: PathCache = Mutex::new(None);

/// Binary locations found by `resolve_binary`, by provider id and PATH it was found with. Misses are not kept, so
/// installing a CLI is seen on the next turn.
type BinaryCache = Mutex<Vec<(String, Option<OsString>, Instant, PathBuf)>>;
static BINARY_CACHE: BinaryCache = Mutex::new(Vec::new());

fn build_augmented_path() -> Option<OsString> {
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
    if let Some(path) = pinned {
        return path.into_os_string();
    }
    // Every turn asks: the scans below stat dozens of paths (slow behind antivirus on Windows). Reuse a hit while
    // the file is still there.
    let current_path = std::env::var_os("PATH");
    if let Ok(cache) = BINARY_CACHE.lock() {
        let hit = cache.iter().find(|(id, key, at, path)| {
            id == provider.id()
                && *key == current_path
                && at.elapsed() < CACHE_TTL
                && path.is_file()
        });
        if let Some((_, _, _, path)) = hit {
            return path.clone().into_os_string();
        }
    }
    let found = provider
        .candidates()
        .into_iter()
        .find(|path| path.is_file())
        .or_else(from_path)
        .or_else(from_common)
        .or_else(from_shell);
    match found {
        Some(path) => {
            if let Ok(mut cache) = BINARY_CACHE.lock() {
                cache.retain(|(id, ..)| id != provider.id());
                cache.push((
                    provider.id().to_string(),
                    current_path,
                    Instant::now(),
                    path.clone(),
                ));
            }
            path.into_os_string()
        }
        None => provider.binary().into(),
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

    struct Located(PathBuf);
    impl Provider for Located {
        fn id(&self) -> &'static str {
            "resolve-cache-test"
        }
        fn binary(&self) -> &'static str {
            "resolve-cache-test-bin"
        }
        fn candidates(&self) -> Vec<PathBuf> {
            vec![self.0.clone()]
        }
        fn args(&self, _req: &super::super::TurnRequest) -> Vec<String> {
            Vec::new()
        }
        fn parse_line(&self, _line: &str) -> Vec<super::super::AgentEvent> {
            Vec::new()
        }
    }

    #[test]
    fn a_found_binary_is_reused_until_it_disappears() {
        let file = std::env::temp_dir().join(format!("ag-resolve-cache-{}", std::process::id()));
        std::fs::write(&file, "").unwrap();
        let provider = Located(file.clone());
        assert_eq!(resolve_binary(&provider), file.clone().into_os_string());
        // Served from the cache even when the provider no longer lists it.
        let moved = Located(PathBuf::from("/nowhere"));
        assert_eq!(resolve_binary(&moved), file.clone().into_os_string());
        // A removed file is not returned from the cache.
        std::fs::remove_file(&file).unwrap();
        assert_eq!(resolve_binary(&moved), OsString::from("resolve-cache-test-bin"));
    }

    #[test]
    fn augmented_path_is_stable_between_calls() {
        assert_eq!(augmented_path(), augmented_path());
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
