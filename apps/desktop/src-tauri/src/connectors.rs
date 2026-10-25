//! Settings → Connectors: finds the agent CLIs on this machine, installs and signs them in, and switches between
//! the accounts each one has been signed in with.
//!
//! Account switching is done by two existing tools instead of our own credential juggling:
//! - Codex: `codex-auth` (`@loongphy/codex-auth`). Its `~/.codex/accounts/registry.json` is read directly (it holds
//!   no tokens); only changes go through the CLI.
//! - Claude Code: `cswap` (`claude-swap`, installed with `uv tool`). Its `--json` output is parsed.
//!
//! Nothing here reads or prints a token. Every external command runs without a shell, with the arguments given
//! separately, and without a console window.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

// ---------------------------------------------------------------------------------------------------------------
// Finding and running programs

/// Places a desktop app started without a shell PATH still finds the CLIs.
fn extra_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(home) = crate::home_folder() {
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".cargo").join("bin"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        dirs.push(local.join("pnpm"));
        dirs.push(local.join("agy").join("bin"));
        if let Ok(versions) = std::fs::read_dir(local.join("mise").join("installs").join("node")) {
            dirs.extend(versions.flatten().map(|entry| entry.path()));
        }
    }
    if let Some(roaming) = std::env::var_os("APPDATA").map(PathBuf::from) {
        dirs.push(roaming.join("npm"));
    }
    dirs
}

fn search_path() -> Vec<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).chain(extra_dirs()).collect()
}

/// The first `<dir>/<name>` that exists, trying the executable suffixes of the platform.
pub(crate) fn which(name: &str) -> Option<PathBuf> {
    let suffixes: &[&str] = if cfg!(windows) { &[".exe", ".cmd", ".bat"] } else { &[""] };
    search_path().into_iter().find_map(|dir| {
        suffixes
            .iter()
            .map(|suffix| dir.join(format!("{name}{suffix}")))
            .find(|path| path.is_file())
    })
}

/// A command with no console window and no stdin, whose PATH also holds the well-known tool folders (the `.cmd`
/// shims of npm tools need `node` on it).
pub(crate) fn command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Ok(path) = std::env::join_paths(search_path()) {
        command.env("PATH", path);
    }
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    command
}

pub(crate) struct Captured {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

pub(crate) async fn capture(program: &Path, args: &[&str], limit: Duration) -> Result<Captured, String> {
    let child = command(program)
        .args(args)
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", program.display()))?;
    let output = tokio::time::timeout(limit, child.wait_with_output())
        .await
        .map_err(|_| format!("{} took longer than {}s", program.display(), limit.as_secs()))?
        .map_err(|error| error.to_string())?;
    Ok(Captured {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// The first non-empty line of a tool's `--version`.
async fn version_of(program: &Path) -> Option<String> {
    let out = capture(program, &["--version"], Duration::from_secs(8)).await.ok()?;
    let text = if out.stdout.trim().is_empty() { out.stderr } else { out.stdout };
    text.lines().map(str::trim).find(|line| !line.is_empty()).map(str::to_owned)
}

/// A line of an install or sign-in that is running, for the log under the button.
#[derive(Clone, Serialize)]
struct LogLine {
    id: String,
    line: String,
}

async fn pump<R: AsyncRead + Unpin>(app: AppHandle, id: String, stream: R) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim_end().to_owned();
        if !line.is_empty() {
            let _ = app.emit("connector-log", LogLine { id: id.clone(), line });
        }
    }
}

/// Runs a command to its end, sending each output line to the UI as `connector-log`.
async fn run_logged(app: &AppHandle, id: &str, program: &Path, args: &[&str], limit: Duration) -> Result<(), String> {
    let _ = app.emit(
        "connector-log",
        LogLine {
            id: id.to_owned(),
            line: format!("$ {} {}", program.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(), args.join(" ")),
        },
    );
    let mut child = command(program)
        .args(args)
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", program.display()))?;
    let out = child.stdout.take().map(|stream| tauri::async_runtime::spawn(pump(app.clone(), id.to_owned(), stream)));
    let err = child.stderr.take().map(|stream| tauri::async_runtime::spawn(pump(app.clone(), id.to_owned(), stream)));
    let status = tokio::time::timeout(limit, child.wait())
        .await
        .map_err(|_| format!("timed out after {} minutes", limit.as_secs() / 60))?
        .map_err(|error| error.to_string())?;
    for task in [out, err].into_iter().flatten() {
        let _ = task.await;
    }
    if status.success() {
        Ok(())
    } else {
        Err(format!("exited with {status}"))
    }
}

// ---------------------------------------------------------------------------------------------------------------
// What is installed and who is signed in

#[derive(Serialize)]
pub struct Tool {
    id: &'static str,
    path: Option<String>,
    version: Option<String>,
}

#[derive(Serialize, Default)]
pub struct Login {
    /// `None` when AgentDeck cannot tell (Antigravity has no status command).
    logged_in: Option<bool>,
    email: Option<String>,
    plan: Option<String>,
    detail: Option<String>,
}

#[derive(Serialize)]
pub struct Detected {
    tools: Vec<Tool>,
    claude: Login,
    codex: Login,
    antigravity: Login,
    opencode: Login,
}

const TOOLS: [&str; 8] = ["claude", "codex", "agy", "opencode", "codex-auth", "cswap", "uv", "npm"];

pub(crate) fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| crate::home_folder().ok().map(|home| home.join(".codex")))
        .unwrap_or_default()
}

fn claude_login(status: &str) -> Login {
    let Ok(json) = serde_json::from_str::<Value>(status) else {
        return Login { logged_in: Some(false), ..Login::default() };
    };
    let text = |key: &str| json[key].as_str().filter(|value| !value.is_empty()).map(str::to_owned);
    Login {
        logged_in: Some(json["loggedIn"].as_bool().unwrap_or(false)),
        email: text("email"),
        plan: text("subscriptionType"),
        detail: text("orgName").or_else(|| text("authMethod")),
    }
}

fn codex_login() -> Login {
    let home = codex_home();
    if let Some(account) = read_codex_registry(&home).ok().and_then(|(accounts, _)| accounts.into_iter().find(|a| a.active)) {
        return Login { logged_in: Some(true), email: Some(account.email), plan: account.plan, detail: None };
    }
    Login { logged_in: Some(home.join("auth.json").is_file()), ..Login::default() }
}

/// Antigravity has no status command and keeps its login in the OS credential store, so the state is read from its
/// own log: the later of "You are not logged into Antigravity" and "Experiments refreshed after login" wins.
fn antigravity_state(log_tail: &str) -> Option<bool> {
    let out = log_tail.rfind("You are not logged into Antigravity");
    let inn = log_tail.rfind("refreshed after login");
    match (inn, out) {
        (Some(inn), Some(out)) => Some(inn > out),
        (Some(_), None) => Some(true),
        (None, Some(_)) => Some(false),
        (None, None) => None,
    }
}

/// The last `authenticated successfully as <email>` in a log.
fn antigravity_email(log_tail: &str) -> Option<String> {
    const MARK: &str = "authenticated successfully as ";
    let rest = &log_tail[log_tail.rfind(MARK)? + MARK.len()..];
    let email: String = rest.chars().take_while(|c| !c.is_whitespace() && *c != ',').collect();
    email.contains('@').then_some(email)
}

fn tail_of(path: &Path) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else { return String::new() };
    let length = file.metadata().map_or(0, |meta| meta.len());
    let _ = file.seek(SeekFrom::Start(length.saturating_sub(512 * 1024)));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

fn antigravity_login() -> Login {
    let Ok(home) = crate::home_folder() else { return Login::default() };
    let folder = home.join(".gemini").join("antigravity-cli").join("log");
    let mut logs: Vec<_> = std::fs::read_dir(folder)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("cli-"))
        .filter_map(|entry| Some((entry.metadata().ok()?.modified().ok()?, entry.path())))
        .collect();
    logs.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    let Some((_, newest)) = logs.first() else { return Login::default() };
    let logged_in = antigravity_state(&tail_of(newest));
    // The sign-in line is written once per sign-in, so it may sit in an older log.
    let email = logs.iter().take(8).find_map(|(_, path)| antigravity_email(&tail_of(path)));
    Login { logged_in, email, ..Login::default() }
}

/// Which CLIs are installed (with versions) and whether each is signed in. Reads no tokens.
#[tauri::command]
pub async fn connectors_detect() -> Detected {
    let handles: Vec<_> = TOOLS
        .iter()
        .map(|id| {
            let found = which(id);
            tauri::async_runtime::spawn(async move {
                let version = match &found {
                    Some(path) => version_of(path).await,
                    None => None,
                };
                Tool { id, path: found.map(|path| path.to_string_lossy().into_owned()), version }
            })
        })
        .collect();
    let mut tools = Vec::new();
    for handle in handles {
        if let Ok(tool) = handle.await {
            tools.push(tool);
        }
    }
    let claude = match which("claude") {
        Some(path) => match capture(&path, &["auth", "status"], Duration::from_secs(10)).await {
            Ok(out) => claude_login(&out.stdout),
            Err(_) => Login::default(),
        },
        None => Login::default(),
    };
    let opencode = match which("opencode") {
        Some(path) => match capture(&path, &["auth", "list", "--format", "json"], Duration::from_secs(10)).await {
            Ok(out) => Login { logged_in: Some(out.ok), detail: (!out.stdout.trim().is_empty()).then_some("OpenCode auth available".into()), ..Login::default() },
            Err(_) => Login::default(),
        },
        None => Login::default(),
    };
    Detected { tools, claude, codex: codex_login(), antigravity: antigravity_login(), opencode }
}

// ---------------------------------------------------------------------------------------------------------------
// Install and sign in

/// Installs a CLI or one of the helper tools, logging to `connector-log`.
#[tauri::command]
pub async fn connector_install(app: AppHandle, id: String) -> Result<(), String> {
    let limit = Duration::from_secs(600);
    let node_tool = |package: &str| -> Result<(PathBuf, Vec<String>), String> {
        if let Some(npm) = which("npm") {
            Ok((npm, vec!["install".into(), "-g".into(), package.into()]))
        } else if let Some(pnpm) = which("pnpm") {
            Ok((pnpm, vec!["add".into(), "-g".into(), package.into()]))
        } else {
            Err("Node.js (npm) is needed for this: install it from nodejs.org first".to_string())
        }
    };
    let (program, args): (PathBuf, Vec<String>) = match id.as_str() {
        "codex" => node_tool("@openai/codex")?,
        "opencode" => node_tool("@opencode/cli")?,
        "codex-auth" => node_tool("@loongphy/codex-auth")?,
        "cswap" => {
            let uv = which("uv").ok_or("`uv` is needed for claude-swap: install it from docs.astral.sh/uv first")?;
            (uv, vec!["tool".into(), "install".into(), "claude-swap".into()])
        }
        "claude" => {
            // Anthropic's own installer script.
            if cfg!(windows) {
                let shell = which("pwsh").or_else(|| which("powershell")).ok_or("PowerShell was not found")?;
                (
                    shell,
                    vec![
                        "-NoProfile".into(),
                        "-ExecutionPolicy".into(),
                        "Bypass".into(),
                        "-Command".into(),
                        "irm https://claude.ai/install.ps1 | iex".into(),
                    ],
                )
            } else {
                let shell = which("bash").ok_or("bash was not found")?;
                (shell, vec!["-c".into(), "curl -fsSL https://claude.ai/install.sh | bash".into()])
            }
        }
        "agy" => {
            if cfg!(windows) {
                let shell = which("pwsh").or_else(|| which("powershell")).ok_or("PowerShell was not found")?;
                (shell, vec!["-NoProfile".into(), "-ExecutionPolicy".into(), "Bypass".into(), "-Command".into(), "irm https://antigravity.google/cli/install.ps1 | iex".into()])
            } else {
                let shell = which("bash").ok_or("bash was not found")?;
                (shell, vec!["-c".into(), "curl -fsSL https://antigravity.google/cli/install.sh | bash".into()])
            }
        }
        other => return Err(format!("do not know how to install `{other}`")),
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_logged(&app, &id, &program, &args, limit).await
}

/// Opens the vendor's sign-in (a browser page) and waits for it to finish. Codex goes through `codex-auth login`
/// when it is installed, so the account is also added to its list.
#[tauri::command]
pub async fn connector_login(app: AppHandle, id: String) -> Result<(), String> {
    let limit = Duration::from_secs(300);
    match id.as_str() {
        "claude" => {
            let claude = which("claude").ok_or("Claude Code is not installed")?;
            run_logged(&app, &id, &claude, &["auth", "login"], limit).await
        }
        "codex" => match (which("codex-auth"), which("codex")) {
            (Some(auth), _) => run_logged(&app, &id, &auth, &["login"], limit).await,
            (None, Some(codex)) => run_logged(&app, &id, &codex, &["login"], limit).await,
            (None, None) => Err("Codex is not installed".to_string()),
        },
        "opencode" => {
            let opencode = which("opencode").ok_or("OpenCode is not installed")?;
            // AgentDeck has no interactive TTY. Passing the OpenAI integration skips OpenCode's provider picker, and
            // the method is named explicitly: without a TTY OpenCode refuses to choose one by itself ("Pass --method").
            run_logged(&app, &id, &opencode, &["auth", "login", "openai", "--method", "chatgpt-browser"], limit).await
        }
        "agy" => {
            // No headless sign-in: start `agy` in its own window, where it asks.
            let agy = which("agy").ok_or("Antigravity is not installed")?;
            if !cfg!(windows) {
                return Err("run `agy` in a terminal and sign in there".to_string());
            }
            std::process::Command::new("cmd")
                .args(["/C", "start", "Antigravity sign-in", &agy.to_string_lossy()])
                .spawn()
                .map(|_| ())
                .map_err(|error| error.to_string())
        }
        other => Err(format!("no sign-in for `{other}`")),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Accounts

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct UsageWindow {
    /// `5h`, `7d`, `30d`…
    label: String,
    pct: f64,
    /// Epoch seconds. May be in the past when the figure is old.
    resets_at: Option<i64>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Account {
    /// What to pass back to `account_switch`.
    id: String,
    email: String,
    alias: Option<String>,
    plan: Option<String>,
    active: bool,
    usage: Vec<UsageWindow>,
    /// Epoch seconds of the figures, when known.
    observed_at: Option<i64>,
    /// Why there is no usage (expired login, API off).
    status: Option<String>,
    /// The login has expired (the usage call answered 401): the account must sign in again and cannot be switched to.
    expired: bool,
}

#[derive(Serialize)]
pub struct AutoSwitch {
    enabled: bool,
    five_hour: u64,
    weekly: u64,
}

#[derive(Serialize)]
pub struct Accounts {
    /// The switcher tool is installed.
    manager: bool,
    accounts: Vec<Account>,
    auto_switch: Option<AutoSwitch>,
    error: Option<String>,
}

fn window_label(minutes: i64) -> String {
    match minutes {
        300 => "5h".to_string(),
        10_080 => "7d".to_string(),
        43_200 => "30d".to_string(),
        m if m % 1440 == 0 => format!("{}d", m / 1440),
        m if m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{m}m"),
    }
}

/// Emails `codex-auth list` last showed with a 401 (signed out). The registry keeps no trace of that, only the table
/// the tool prints, so the answer of the latest refresh is remembered here.
static CODEX_EXPIRED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Rows of the `codex-auth list` table whose usage columns say `401`.
fn expired_in_list(table: &str) -> Vec<String> {
    table
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            words.next()?.parse::<u32>().ok()?;
            let email = words.next()?;
            words.any(|word| word == "401").then(|| email.to_owned())
        })
        .collect()
}

/// Accounts of `codex-auth`, from its registry file (no tokens in it).
fn read_codex_registry(codex_home: &Path) -> Result<(Vec<Account>, Option<AutoSwitch>), String> {
    let path = codex_home.join("accounts").join("registry.json");
    let text = std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    parse_codex_registry(&text)
}

fn parse_codex_registry(text: &str) -> Result<(Vec<Account>, Option<AutoSwitch>), String> {
    let json: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let active = json["active_account_key"].as_str().unwrap_or_default();
    let accounts = json["accounts"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let key = item["account_key"].as_str()?;
                    let usage = &item["last_usage"];
                    let windows: Vec<UsageWindow> = ["primary", "secondary"]
                        .iter()
                        .filter_map(|name| {
                            let window = &usage[*name];
                            Some(UsageWindow {
                                label: window_label(window["window_minutes"].as_i64()?),
                                pct: window["used_percent"].as_f64()?,
                                resets_at: window["resets_at"].as_i64(),
                            })
                        })
                        .collect();
                    let text = |name: &str| item[name].as_str().filter(|value| !value.is_empty()).map(str::to_owned);
                    Some(Account {
                        expired: false,
                        id: text("email").unwrap_or_else(|| key.to_owned()),
                        email: text("email").unwrap_or_else(|| key.to_owned()),
                        alias: text("alias"),
                        plan: text("plan"),
                        active: key == active,
                        status: windows.is_empty().then(|| "no usage figures yet".to_string()),
                        usage: windows,
                        observed_at: item["last_usage_at"].as_i64(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let auto = &json["auto_switch"];
    let auto_switch = auto["enabled"].as_bool().map(|enabled| AutoSwitch {
        enabled,
        five_hour: auto["threshold_5h_percent"].as_u64().unwrap_or(10),
        weekly: auto["threshold_weekly_percent"].as_u64().unwrap_or(5),
    });
    Ok((accounts, auto_switch))
}

/// `YYYY-MM-DDTHH:MM:SSZ` to epoch seconds (the offset is taken to be UTC, which is what `cswap` writes).
fn parse_iso(text: &str) -> Option<i64> {
    let number = |from: usize, len: usize| text.get(from..from + len)?.parse::<i64>().ok();
    let (year, month, day) = (number(0, 4)?, number(5, 2)?, number(8, 2)?);
    let (hour, minute, second) = (number(11, 2)?, number(14, 2)?, number(17, 2)?);
    // Days since 1970-01-01 (Howard Hinnant's civil-from-days, inverted).
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = shifted.div_euclid(400);
    let year_of_era = shifted - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

/// `cswap list --json`.
fn parse_cswap(text: &str) -> Result<Vec<Account>, String> {
    let json: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if let Some(error) = json.get("error") {
        return Err(error["message"].as_str().map_or_else(|| error.to_string(), str::to_owned));
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    Ok(json["accounts"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|item| {
                    let number = item["number"].as_i64()?;
                    let windows: Vec<UsageWindow> = [("fiveHour", "5h"), ("sevenDay", "7d")]
                        .iter()
                        .filter_map(|(key, label)| {
                            let window = &item["usage"][*key];
                            Some(UsageWindow {
                                label: (*label).to_string(),
                                pct: window["pct"].as_f64()?,
                                resets_at: window["resetsAt"].as_str().and_then(parse_iso),
                            })
                        })
                        .collect();
                    let status = item["usageStatus"].as_str().filter(|status| *status != "ok").map(str::to_owned);
                    Some(Account {
                        expired: status.as_deref() == Some("expired"),
                        id: number.to_string(),
                        email: item["email"].as_str().unwrap_or("(unknown)").to_owned(),
                        alias: item["alias"].as_str().filter(|alias| !alias.is_empty()).map(str::to_owned),
                        plan: item["plan"].as_str().map(str::to_owned),
                        active: item["active"].as_bool().unwrap_or(false),
                        observed_at: (!windows.is_empty()).then_some(now),
                        usage: windows,
                        status,
                    })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// The accounts one provider has been signed in with. `refresh` asks the switcher tool for fresh usage figures
/// (a network call by that tool); without it the saved figures are read from disk.
#[tauri::command]
pub async fn accounts_list(provider: String, refresh: bool) -> Result<Accounts, String> {
    match provider.as_str() {
        "codex" => {
            let manager = which("codex-auth");
            if refresh {
                if let Some(auth) = &manager {
                    // `list` refreshes the usage figures in the registry as a side effect.
                    if let Ok(out) = capture(auth, &["list"], Duration::from_secs(40)).await {
                        if let Ok(mut known) = CODEX_EXPIRED.lock() {
                            *known = expired_in_list(&out.stdout);
                        }
                    }
                }
            }
            match read_codex_registry(&codex_home()) {
                Ok((mut accounts, auto_switch)) => {
                    let known = CODEX_EXPIRED.lock().map(|known| known.clone()).unwrap_or_default();
                    for account in accounts.iter_mut().filter(|account| known.contains(&account.email)) {
                        account.expired = true;
                        account.usage.clear();
                        account.status = Some("signed out (401)".to_string());
                    }
                    Ok(Accounts { manager: manager.is_some(), accounts, auto_switch, error: None })
                }
                Err(error) => Ok(Accounts {
                    manager: manager.is_some(),
                    accounts: Vec::new(),
                    auto_switch: None,
                    error: manager.is_some().then_some(error),
                }),
            }
        }
        "claude" => {
            let Some(cswap) = which("cswap") else {
                return Ok(Accounts { manager: false, accounts: Vec::new(), auto_switch: None, error: None });
            };
            match capture(&cswap, &["list", "--json"], Duration::from_secs(40)).await {
                Ok(out) => match parse_cswap(&out.stdout) {
                    Ok(accounts) => Ok(Accounts { manager: true, accounts, auto_switch: None, error: None }),
                    Err(error) => Ok(Accounts { manager: true, accounts: Vec::new(), auto_switch: None, error: Some(error) }),
                },
                Err(error) => Ok(Accounts { manager: true, accounts: Vec::new(), auto_switch: None, error: Some(error) }),
            }
        }
        other => Err(format!("`{other}` has no account list")),
    }
}

fn safe_arg(value: &str) -> Result<&str, String> {
    if value.is_empty() || value.starts_with('-') || value.chars().any(char::is_control) {
        Err("that is not a valid account".to_string())
    } else {
        Ok(value)
    }
}

pub(crate) fn failure(out: &Captured) -> String {
    let text = if out.stderr.trim().is_empty() { &out.stdout } else { &out.stderr };
    text.lines().map(str::trim).rfind(|line| !line.is_empty()).unwrap_or("the command failed").to_owned()
}

/// Makes `id` the account the CLI uses from now on. A turn already running keeps the account it started with.
#[tauri::command]
pub async fn account_switch(provider: String, id: String) -> Result<(), String> {
    let id = safe_arg(&id)?;
    let (tool, args): (&str, Vec<&str>) = match provider.as_str() {
        "codex" => ("codex-auth", vec!["switch", id]),
        "claude" => ("cswap", vec!["switch", id, "--json"]),
        other => return Err(format!("`{other}` has no accounts to switch")),
    };
    let program = which(tool).ok_or_else(|| format!("`{tool}` is not installed"))?;
    let out = capture(&program, &args, Duration::from_secs(30)).await?;
    if out.ok {
        Ok(())
    } else {
        Err(failure(&out))
    }
}

/// Signs in (browser) and saves the account into the switcher's list.
#[tauri::command]
pub async fn account_add(app: AppHandle, provider: String) -> Result<(), String> {
    match provider.as_str() {
        "codex" => {
            let auth = which("codex-auth").ok_or("`codex-auth` is not installed")?;
            run_logged(&app, "codex", &auth, &["login"], Duration::from_secs(300)).await
        }
        "claude" => {
            let cswap = which("cswap").ok_or("`cswap` is not installed")?;
            let claude = which("claude").ok_or("Claude Code is not installed")?;
            // Sign in as the new account first; the switcher then saves whoever is signed in.
            run_logged(&app, "claude", &claude, &["auth", "login"], Duration::from_secs(300)).await?;
            run_logged(&app, "claude", &cswap, &["add"], Duration::from_secs(60)).await
        }
        other => Err(format!("`{other}` has no accounts to add")),
    }
}

/// Saves the account Claude Code is signed in as right now into `cswap`'s list, without a new sign-in.
#[tauri::command]
pub async fn account_adopt() -> Result<(), String> {
    let cswap = which("cswap").ok_or("`cswap` is not installed")?;
    let out = capture(&cswap, &["add"], Duration::from_secs(60)).await?;
    if out.ok {
        Ok(())
    } else {
        Err(failure(&out))
    }
}

/// Removes a saved Codex account from `codex-auth`'s list.
#[tauri::command]
pub async fn account_remove(provider: String, id: String) -> Result<(), String> {
    if provider != "codex" {
        return Err("only Codex accounts can be removed here".to_string());
    }
    let id = safe_arg(&id)?;
    let auth = which("codex-auth").ok_or("`codex-auth` is not installed")?;
    let out = capture(&auth, &["remove", id], Duration::from_secs(30)).await?;
    if out.ok {
        Ok(())
    } else {
        Err(failure(&out))
    }
}

/// `codex-auth`'s own background switch when a window runs low.
#[tauri::command]
pub async fn codex_auto_switch(enabled: bool, five_hour: u8, weekly: u8) -> Result<(), String> {
    let auth = which("codex-auth").ok_or("`codex-auth` is not installed")?;
    let five = five_hour.min(99).to_string();
    let week = weekly.min(99).to_string();
    let thresholds = capture(&auth, &["config", "auto", "--5h", &five, "--weekly", &week], Duration::from_secs(20)).await?;
    if !thresholds.ok {
        return Err(failure(&thresholds));
    }
    let toggle = capture(&auth, &["config", "auto", if enabled { "enable" } else { "disable" }], Duration::from_secs(20)).await?;
    if toggle.ok {
        Ok(())
    } else {
        Err(failure(&toggle))
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Antigravity usage

#[derive(Serialize, Debug, PartialEq)]
pub struct QuotaGroup {
    id: String,
    label: String,
    windows: Vec<UsageWindow>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct AgyQuota {
    /// Epoch seconds of the figures.
    observed_at: Option<i64>,
    groups: Vec<QuotaGroup>,
}

/// agy hands its quota to the status-line command, and a tool that already owns that hook (AI Usage Meter) saves
/// it to `usage-cache.json`: `{"fetchedAt": ms, "quota": {"gemini-5h": {"remaining_fraction", "reset_time"}, ...}}`.
/// The keys are `<model group>-<window>`. Read-only: AgentDeck never edits agy's status-line setting.
fn parse_agy_quota(text: &str) -> Option<AgyQuota> {
    let json: Value = serde_json::from_str(text).ok()?;
    let quota = json["quota"].as_object()?;
    let mut groups: Vec<QuotaGroup> = Vec::new();
    for (key, entry) in quota {
        let (group, window) = key.rsplit_once('-')?;
        let Some(remaining) = entry["remaining_fraction"].as_f64() else { continue };
        let window = UsageWindow {
            label: if window == "weekly" { "7d".to_string() } else { window.to_string() },
            pct: ((1.0 - remaining) * 100.0).clamp(0.0, 100.0),
            resets_at: entry["reset_time"].as_str().and_then(parse_iso),
        };
        match groups.iter_mut().find(|item| item.id == group) {
            Some(item) => item.windows.push(window),
            None => groups.push(QuotaGroup {
                id: group.to_string(),
                label: match group {
                    "gemini" => "Gemini models".to_string(),
                    "3p" => "Claude and GPT-OSS models".to_string(),
                    other => other.to_string(),
                },
                windows: vec![window],
            }),
        }
    }
    groups.sort_by_key(|item| item.id != "gemini");
    for group in &mut groups {
        group.windows.sort_by_key(|window| window.label != "5h");
    }
    Some(AgyQuota { observed_at: json["fetchedAt"].as_i64().map(|ms| ms / 1000), groups })
}

/// The last quota figures agy reported, if a tool saved them. `None` when nothing is there.
#[tauri::command]
pub fn antigravity_quota() -> Option<AgyQuota> {
    let path = crate::home_folder().ok()?.join(".gemini").join("antigravity-cli").join("ai-quota-tray").join("usage-cache.json");
    parse_agy_quota(&std::fs::read_to_string(path).ok()?)
}

// ---------------------------------------------------------------------------------------------------------------
// Antigravity permission modes

/// agy prints this and carries on in its default mode when the account may not use `--mode <name>`.
fn mode_unavailable(output: &str) -> bool {
    output.contains("is not available for this account")
}

#[derive(Serialize, Debug, Default, PartialEq)]
pub struct AgyModes {
    /// `--mode` values the account cannot use (`auto`, `accept-edits`, `plan`). Empty: all work, or it could not be told.
    unavailable: Vec<String>,
}

/// Which of agy's `--mode` values this account can use. agy only says so when it starts a turn, so each mode is tried
/// once with a one-word prompt that is cut after 3 seconds; the answer is kept per agy version in
/// `~/.agentdeck/agy-modes.json`, so it is asked again only after an update (or with `force`).
#[tauri::command]
pub async fn agy_modes(force: bool) -> AgyModes {
    let Some(agy) = which("agy") else { return AgyModes::default() };
    let version = version_of(&agy).await.unwrap_or_default();
    let cache = crate::home_folder().ok().map(|home| home.join(".agentdeck").join("agy-modes.json"));
    if !force {
        let saved = cache.as_ref().and_then(|path| std::fs::read_to_string(path).ok()).and_then(|text| serde_json::from_str::<Value>(&text).ok());
        if let Some(saved) = saved.filter(|saved| saved["version"].as_str() == Some(version.as_str())) {
            let unavailable = saved["unavailable"].as_array().map(|list| list.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect()).unwrap_or_default();
            return AgyModes { unavailable };
        }
    }
    let probe = |mode: &'static str| {
        let agy = agy.clone();
        async move {
            let args = ["-p", ".", "--mode", mode, "--print-timeout", "3s", "--output-format", "text"];
            match capture(&agy, &args, Duration::from_secs(30)).await {
                Ok(out) if mode_unavailable(&format!("{}{}", out.stdout, out.stderr)) => Some(mode.to_owned()),
                _ => None,
            }
        }
    };
    let (auto, edits, plan) = tokio::join!(probe("auto"), probe("accept-edits"), probe("plan"));
    let unavailable: Vec<String> = [auto, edits, plan].into_iter().flatten().collect();
    if let Some(path) = cache {
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let _ = std::fs::write(path, serde_json::json!({ "version": version, "unavailable": unavailable }).to_string());
    }
    AgyModes { unavailable }
}

// ---------------------------------------------------------------------------------------------------------------
// Custom and local providers

#[derive(Serialize)]
pub struct Probe {
    ok: bool,
    models: Vec<String>,
    ms: u64,
    error: Option<String>,
}

fn models_url(kind: &str, base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    match kind {
        "ollama" => format!("{base}/api/tags"),
        "anthropic" => format!("{base}/v1/models"),
        _ => format!("{base}/models"),
    }
}

fn parse_models(kind: &str, json: &Value) -> Vec<String> {
    let (list, key) = if kind == "ollama" { (&json["models"], "name") } else { (&json["data"], "id") };
    list.as_array()
        .map(|items| items.iter().filter_map(|item| item[key].as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

async fn probe(kind: &str, base: &str, key: Option<&str>, key_env: Option<&str>, limit: Duration) -> Probe {
    let started = Instant::now();
    let done = |models: Vec<String>, error: Option<String>| Probe {
        ok: error.is_none(),
        models,
        ms: started.elapsed().as_millis() as u64,
        error,
    };
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return done(Vec::new(), Some("the address must start with http:// or https://".to_string()));
    }
    let client = match reqwest::Client::builder().timeout(limit).build() {
        Ok(client) => client,
        Err(error) => return done(Vec::new(), Some(error.to_string())),
    };
    let mut request = client.get(models_url(kind, base));
    // The key typed in the settings wins; otherwise the environment variable the user named.
    let key = match (key.filter(|key| !key.is_empty()), key_env.filter(|name| !name.is_empty())) {
        (Some(key), _) => Some(key.to_owned()),
        (None, Some(name)) => match std::env::var(name) {
            Ok(key) => Some(key),
            Err(_) => return done(Vec::new(), Some(format!("environment variable {name} is not set"))),
        },
        (None, None) => None,
    };
    if let Some(key) = key {
        request = if kind == "anthropic" { request.header("x-api-key", key).header("anthropic-version", "2023-06-01") } else { request.bearer_auth(key) };
    }
    match request.send().await {
        Ok(response) if response.status().is_success() => match response.json::<Value>().await {
            Ok(json) => done(parse_models(kind, &json), None),
            Err(error) => done(Vec::new(), Some(format!("not a model list: {error}"))),
        },
        Ok(response) => done(Vec::new(), Some(format!("the server answered {}", response.status()))),
        Err(error) if error.is_timeout() => done(Vec::new(), Some("no answer in time".to_string())),
        Err(error) if error.is_connect() => done(Vec::new(), Some("could not connect: is the server running?".to_string())),
        Err(error) => done(Vec::new(), Some(error.to_string())),
    }
}

/// Asks a provider's endpoint for its models: the "Test connection" button. The key, if any, is read from the
/// environment variable the user named and never leaves this function except as a request header.
#[tauri::command]
pub async fn provider_probe(kind: String, base_url: String, api_key: Option<String>, key_env: Option<String>) -> Probe {
    probe(&kind, &base_url, api_key.as_deref(), key_env.as_deref(), Duration::from_secs(6)).await
}

#[derive(Serialize)]
pub struct LocalServer {
    kind: &'static str,
    label: &'static str,
    base_url: &'static str,
    models: Vec<String>,
}

/// Looks for the usual local model servers (Ollama, LM Studio, llama.cpp) on this machine.
#[tauri::command]
pub async fn local_scan() -> Vec<LocalServer> {
    const SERVERS: [(&str, &str, &str); 3] = [
        ("ollama", "Ollama", "http://127.0.0.1:11434"),
        ("openai", "LM Studio", "http://127.0.0.1:1234/v1"),
        ("openai", "llama.cpp", "http://127.0.0.1:8080/v1"),
    ];
    let limit = Duration::from_millis(700);
    let handles: Vec<_> = SERVERS
        .iter()
        .map(|(kind, label, base_url)| {
            tauri::async_runtime::spawn(async move {
                let found = probe(kind, base_url, None, None, limit).await;
                found.ok.then_some(LocalServer { kind, label, base_url, models: found.models })
            })
        })
        .collect();
    let mut found = Vec::new();
    for handle in handles {
        if let Ok(Some(server)) = handle.await {
            found.push(server);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_registry_gives_accounts_and_windows() {
        let text = r#"{
          "active_account_key": "b",
          "auto_switch": {"enabled": true, "threshold_5h_percent": 12, "threshold_weekly_percent": 6},
          "accounts": [
            {"account_key": "a", "email": "a@x.com", "alias": "", "plan": "free",
             "last_usage": {"primary": {"used_percent": 93, "window_minutes": 43200, "resets_at": 1788172584}, "secondary": null},
             "last_usage_at": 1786326361},
            {"account_key": "b", "email": "b@x.com", "plan": "business",
             "last_usage": {"primary": {"used_percent": 91.5, "window_minutes": 300, "resets_at": 5},
                            "secondary": {"used_percent": 95, "window_minutes": 10080, "resets_at": 6}}},
            {"account_key": "c", "email": "c@x.com"}
          ]}"#;
        let (accounts, auto) = parse_codex_registry(text).expect("parses");
        assert_eq!(accounts.len(), 3);
        assert!(!accounts[0].active && accounts[1].active);
        assert_eq!(accounts[0].alias, None);
        assert_eq!(accounts[0].usage, vec![UsageWindow { label: "30d".into(), pct: 93.0, resets_at: Some(1_788_172_584) }]);
        assert_eq!(accounts[1].usage.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), ["5h", "7d"]);
        assert_eq!(accounts[2].status.as_deref(), Some("no usage figures yet"));
        let auto = auto.expect("auto switch");
        assert!(auto.enabled && auto.five_hour == 12 && auto.weekly == 6);
    }

    #[test]
    fn expired_rows_are_found_in_the_list_table() {
        let table = "  ACCOUNT  PLAN  5H USAGE  WEEKLY USAGE
----
01 a@x.com   Free   401   401   74d ago
02 b@x.com   Business   100% (03:03 on 24 Oct)  88% (00:12 on 30 Oct)  Now
";
        assert_eq!(expired_in_list(table), vec!["a@x.com".to_string()]);
    }

    #[test]
    fn cswap_json_gives_accounts() {
        let text = r#"{"schemaVersion":1,"activeAccountNumber":2,"accounts":[
          {"number":1,"email":"a@x.com","active":false,"usageStatus":"expired"},
          {"number":2,"email":"b@x.com","active":true,"usageStatus":"ok",
           "usage":{"fiveHour":{"pct":25.0,"resetsAt":"2026-06-22T23:29:59Z"},"sevenDay":{"pct":16.0,"resetsAt":"2026-06-26T17:59:59Z"}}}]}"#;
        let accounts = parse_cswap(text).expect("parses");
        assert_eq!(accounts[0].status.as_deref(), Some("expired"));
        assert!(accounts[0].expired && !accounts[1].expired);
        assert!(accounts[0].usage.is_empty());
        assert!(accounts[1].active);
        assert_eq!(accounts[1].usage[0].pct, 25.0);
        assert_eq!(accounts[1].usage[0].resets_at, Some(1_782_170_999));
        let error = parse_cswap(r#"{"schemaVersion":1,"error":{"message":"no accounts"}}"#);
        assert_eq!(error, Err("no accounts".to_string()));
    }

    #[test]
    fn iso_dates_become_epoch_seconds() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("2000-03-01T00:00:01Z"), Some(951_868_801));
        assert_eq!(parse_iso("nope"), None);
    }

    #[test]
    fn claude_status_is_read() {
        let login = claude_login(r#"{"loggedIn":true,"authMethod":"claude.ai","email":"me@x.com","orgName":"Org"}"#);
        assert_eq!(login.logged_in, Some(true));
        assert_eq!(login.email.as_deref(), Some("me@x.com"));
        assert_eq!(login.detail.as_deref(), Some("Org"));
        assert_eq!(claude_login("Not logged in").logged_in, Some(false));
    }

    #[test]
    fn model_lists_follow_each_kind() {
        assert_eq!(models_url("ollama", "http://h:1/"), "http://h:1/api/tags");
        assert_eq!(models_url("openai", "http://h/v1"), "http://h/v1/models");
        assert_eq!(models_url("anthropic", "https://api.x.com"), "https://api.x.com/v1/models");
        let tags: Value = serde_json::json!({"models": [{"name": "llama3"}]});
        assert_eq!(parse_models("ollama", &tags), ["llama3"]);
        let data: Value = serde_json::json!({"data": [{"id": "m1"}, {"id": "m2"}]});
        assert_eq!(parse_models("openai", &data), ["m1", "m2"]);
    }

    #[test]
    fn account_arguments_cannot_be_flags() {
        assert!(safe_arg("-rf").is_err());
        assert!(safe_arg("").is_err());
        assert_eq!(safe_arg("a@b.com"), Ok("a@b.com"));
    }

    #[test]
    fn antigravity_state_follows_the_later_log_line() {
        let out = "x You are not logged into Antigravity.
";
        let inn = "y Experiments refreshed after login
";
        assert_eq!(antigravity_state(&format!("{out}{inn}")), Some(true));
        assert_eq!(antigravity_state(&format!("{inn}{out}")), Some(false));
        assert_eq!(antigravity_state(inn), Some(true));
        assert_eq!(antigravity_state("nothing"), None);
    }

    #[test]
    fn antigravity_email_is_the_last_sign_in() {
        let log = "x authenticated successfully as a@x.com
 y authenticated successfully as b@x.com, authMethod=consumer";
        assert_eq!(antigravity_email(log).as_deref(), Some("b@x.com"));
        assert_eq!(antigravity_email("nothing"), None);
    }

    #[test]
    fn agy_quota_groups_by_model_family() {
        let text = r#"{"fetchedAt":1792739564113,"quota":{
          "3p-5h":{"remaining_fraction":1,"reset_time":"2026-10-23T12:11:42Z"},
          "3p-weekly":{"remaining_fraction":0.5,"reset_time":"2026-10-30T07:11:42Z"},
          "gemini-weekly":{"remaining_fraction":0.5658594,"reset_time":"2026-10-24T13:04:48Z"},
          "gemini-5h":{"remaining_fraction":0.98,"reset_time":"2026-10-23T11:57:34Z"}}}"#;
        let quota = parse_agy_quota(text).expect("parses");
        assert_eq!(quota.observed_at, Some(1_792_739_564));
        assert_eq!(quota.groups[0].id, "gemini");
        assert_eq!(quota.groups[0].windows.iter().map(|w| w.label.as_str()).collect::<Vec<_>>(), ["5h", "7d"]);
        assert!((quota.groups[0].windows[1].pct - 43.41406).abs() < 0.001);
        assert_eq!(quota.groups[1].label, "Claude and GPT-OSS models");
        assert_eq!(quota.groups[1].windows[0].pct, 0.0);
        assert_eq!(parse_agy_quota("{}"), None);
    }

    #[test]
    fn agy_says_when_a_mode_is_not_for_this_account() {
        assert!(mode_unavailable("warning: --mode auto is not available for this account; continuing in the default mode."));
        assert!(!mode_unavailable("[agy] print timeout after 3s with turn in progress"));
    }

    #[test]
    fn window_labels() {
        assert_eq!(window_label(300), "5h");
        assert_eq!(window_label(10_080), "7d");
        assert_eq!(window_label(90), "90m");
        assert_eq!(window_label(2880), "2d");
    }
}
