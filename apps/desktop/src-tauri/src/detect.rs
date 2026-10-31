//! How a project can be run, read from its own files, so the Preview can offer a button instead of a typed prompt.
//!
//! Looked at: `package.json` scripts (the project folder, its sub-folders, and `apps/*`, `packages/*`, `services/*`),
//! `.claude/launch.json`, and Django's `manage.py`. The address a server will answer at is only a guess made from the
//! script (its `--port`, its framework's usual port); `run` reads the real one from the server's own output.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RunTarget {
    /// What the button says: `web: dev`, `Django`.
    pub name: String,
    /// The shell command that starts it.
    pub command: String,
    /// The folder it runs in.
    pub cwd: String,
    /// The address it will most likely answer at.
    pub url: Option<String>,
    /// Where the command was found: `package.json`, `launch.json`, `manage.py`.
    pub source: String,
    /// The script file a package.json script runs (`node scripts/dev.mjs`): a wrapper that may do anything, ask
    /// questions included.
    pub wrapper: Option<String>,
    /// The wrapper reads from the keyboard (a prompt library, readline): it needs a terminal.
    pub prompts: bool,
}

const SKIP_DIRS: [&str; 8] = [
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "coverage",
    "venv",
    "__pycache__",
];
const SKIP_WORDS: [&str; 10] = [
    "build", "test", "lint", "deploy", "format", "check", "clean", "publish", "release", "e2e",
];
/// Script names that start something worth looking at, best first.
const RANK: [&str; 8] = [
    "dev",
    "start",
    "serve",
    "preview",
    "web",
    "storybook",
    "docs",
    "watch",
];
const MAX: usize = 24;

/// Every way to run the project in `root`, the likeliest first.
pub fn detect(root: &str) -> Vec<RunTarget> {
    let root = PathBuf::from(root);
    let mut found = Vec::new();
    if !root.is_dir() {
        return found;
    }
    launch_json(&root, &mut found);
    for folder in package_folders(&root) {
        packages(&root, &folder, &mut found);
    }
    django(&root, &mut found);
    let mut seen = std::collections::HashSet::new();
    found.retain(|target| seen.insert((target.cwd.clone(), target.command.clone())));
    found.truncate(MAX);
    found
}

/// The project folder, its sub-folders, and the sub-folders of the usual monorepo folders.
fn package_folders(root: &Path) -> Vec<PathBuf> {
    let mut folders = vec![root.to_path_buf()];
    let children = |dir: &Path| -> Vec<PathBuf> {
        let mut list: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .collect()
            })
            .unwrap_or_default();
        list.retain(|path| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            path.is_dir() && !name.starts_with('.') && !SKIP_DIRS.contains(&name)
        });
        list.sort();
        list
    };
    for child in children(root) {
        let name = child
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        folders.push(child.clone());
        if ["apps", "packages", "services", "examples"].contains(&name.as_str()) {
            folders.extend(children(&child));
        }
    }
    folders.retain(|folder| folder.join("package.json").is_file());
    folders
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// `npm run x`, `pnpm run x`, `yarn x` or `bun run x`, by the lock file nearest the folder.
fn runner(root: &Path, folder: &Path, script: &str) -> String {
    let mut dir = Some(folder);
    while let Some(here) = dir {
        for (file, program) in [
            ("pnpm-lock.yaml", "pnpm run"),
            ("yarn.lock", "yarn"),
            ("bun.lockb", "bun run"),
            ("bun.lock", "bun run"),
            ("package-lock.json", "npm run"),
        ] {
            if here.join(file).is_file() {
                return format!("{program} {script}");
            }
        }
        if here == root {
            break;
        }
        dir = here.parent();
    }
    format!("npm run {script}")
}

fn packages(root: &Path, folder: &Path, found: &mut Vec<RunTarget>) {
    let Some(manifest) = read_json(&folder.join("package.json")) else {
        return;
    };
    let Some(scripts) = manifest["scripts"].as_object() else {
        return;
    };
    let dependencies = format!(
        "{} {}",
        manifest["dependencies"], manifest["devDependencies"]
    );
    let package = manifest["name"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| {
            folder
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("app")
                .to_string()
        });
    let mut names: Vec<&String> = scripts
        .keys()
        .filter(|name| {
            let lower = name.to_lowercase();
            let head = lower.split([':', '-']).next().unwrap_or_default();
            RANK.contains(&head) && !SKIP_WORDS.iter().any(|word| lower.contains(word))
        })
        .collect();
    names.sort_by_key(|name| {
        let lower = name.to_lowercase();
        (
            RANK.iter()
                .position(|rank| lower == *rank)
                .unwrap_or(RANK.len()),
            lower,
        )
    });
    for name in names {
        let text = scripts[name].as_str().unwrap_or_default();
        // The desktop app and the packaged ones are not a page.
        if ["tauri", "electron", "expo", "react-native", "nodemon"]
            .iter()
            .any(|word| text.contains(word))
        {
            continue;
        }
        let port = port_of(text, &dependencies, folder);
        let wrapper = wrapper_of(text);
        found.push(RunTarget {
            name: if folder == root {
                name.clone()
            } else {
                format!("{package}: {name}")
            },
            command: runner(root, folder, name),
            cwd: folder.to_string_lossy().into_owned(),
            url: port.map(|port| format!("http://localhost:{port}/")),
            source: "package.json".into(),
            wrapper: wrapper.clone(),
            prompts: wrapper
                .as_deref()
                .is_some_and(|file| uses_prompts(&folder.join(file))),
        });
    }
}

/// The file a script runs when it is `node file.mjs` (a wrapper around the real dev server), as written.
fn wrapper_of(script: &str) -> Option<String> {
    let pattern = Regex::new(r#"^\s*(?:cross-env\s+)?(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+)*node\s+(?:--?[\w-]+(?:=\S+)?\s+)*([^\s&|;<>]+\.(?:mjs|cjs|js))\b"#).ok()?;
    pattern
        .captures(script)
        .map(|found| found[1].trim_matches(['"', '\'']).to_string())
}

/// Whether a script file reads answers from the keyboard (a prompt library, readline, stdin).
fn uses_prompts(file: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(file) else {
        return false;
    };
    let text: String = text.chars().take(200_000).collect();
    Regex::new(r#"(?:from|require\(|import\()\s*['"](?:node:)?(?:inquirer|@inquirer/[\w-]+|enquirer|prompts|@clack/prompts|readline|readline-sync|readline/promises)['"]|process\.stdin"#)
        .is_ok_and(|pattern| pattern.is_match(&text))
}

/// The port a script asks for, else the usual one of the framework it runs.
fn port_of(script: &str, dependencies: &str, folder: &Path) -> Option<u16> {
    let explicit = Regex::new(r"(?:--port[= ]|-p[= ]|PORT=|:)(\d{4,5})\b").ok()?;
    if let Some(port) = explicit
        .captures(script)
        .and_then(|found| found[1].parse().ok())
    {
        return Some(port);
    }
    let uses = |word: &str| script.contains(word) || dependencies.contains(&format!("\"{word}\""));
    if script.contains("vite") || uses("vite") || uses("@sveltejs/kit") {
        return Some(config_port(folder).unwrap_or(5173));
    }
    [
        ("ng serve", 4200),
        ("@angular/cli", 4200),
        ("next", 3000),
        ("nuxt", 3000),
        ("astro", 4321),
        ("react-scripts", 3000),
        ("remix", 3000),
        ("storybook", 6006),
        ("webpack", 8080),
        ("vue-cli-service", 8080),
        ("http-server", 8080),
        ("parcel", 1234),
    ]
    .iter()
    .find(|(word, _)| {
        script.contains(word)
            || (!word.contains(' ') && dependencies.contains(&format!("\"{word}\"")))
    })
    .map(|(_, port)| *port)
}

/// `server: { port: N }` of a vite config, when it has one.
fn config_port(folder: &Path) -> Option<u16> {
    let pattern = Regex::new(r"\bport\s*:\s*(\d{4,5})").ok()?;
    [
        "vite.config.ts",
        "vite.config.js",
        "vite.config.mts",
        "vite.config.mjs",
        "svelte.config.js",
    ]
    .iter()
    .filter_map(|name| std::fs::read_to_string(folder.join(name)).ok())
    .find_map(|text| {
        pattern
            .captures(&text)
            .and_then(|found| found[1].parse().ok())
    })
}

/// The run configurations the project already wrote down (`.claude/launch.json`).
fn launch_json(root: &Path, found: &mut Vec<RunTarget>) {
    let Some(file) = read_json(&root.join(".claude").join("launch.json")) else {
        return;
    };
    for item in file["configurations"].as_array().into_iter().flatten() {
        let Some(program) = item["runtimeExecutable"].as_str() else {
            continue;
        };
        let args: Vec<&str> = item["runtimeArgs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        let port = item["port"].as_u64();
        found.push(RunTarget {
            name: item["name"].as_str().unwrap_or(program).to_string(),
            command: std::iter::once(program)
                .chain(args)
                .collect::<Vec<_>>()
                .join(" "),
            cwd: item["cwd"].as_str().map_or_else(
                || root.to_string_lossy().into_owned(),
                |cwd| root.join(cwd).to_string_lossy().into_owned(),
            ),
            url: item["url"]
                .as_str()
                .map(str::to_string)
                .or_else(|| port.map(|port| format!("http://localhost:{port}/"))),
            source: "launch.json".into(),
            wrapper: None,
            prompts: false,
        });
    }
}

fn django(root: &Path, found: &mut Vec<RunTarget>) {
    if root.join("manage.py").is_file() {
        found.push(RunTarget {
            name: "Django".into(),
            command: if cfg!(windows) {
                "python manage.py runserver"
            } else {
                "python3 manage.py runserver"
            }
            .into(),
            cwd: root.to_string_lossy().into_owned(),
            url: Some("http://localhost:8000/".into()),
            source: "manage.py".into(),
            wrapper: None,
            prompts: false,
        });
    }
}

/// The first local address a server prints about itself (`Local: http://localhost:5173/`), colour codes removed.
pub fn address_in(log: &str) -> Option<String> {
    let colours = Regex::new("\u{1b}\\[[0-9;]*m").ok()?;
    let plain = colours.replace_all(log, "");
    // Taken as printed (scheme, host, port, path): a server started with a certificate answers at https:// only, and
    // one bound to 127.0.0.1 may not answer as localhost. Only an address nobody can browse to becomes localhost.
    let found = Regex::new(
        r#"(https?)://(localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1?\]):(\d{2,5})([^\s)\]'"<>]*)"#,
    )
    .ok()?
    .captures(&plain)?;
    let host = match &found[2] {
        "0.0.0.0" | "[::]" => "localhost",
        other => other,
    };
    let path = found[4].trim_end_matches(['.', ',', ';']);
    Some(format!(
        "{}://{}:{}{}",
        &found[1],
        host,
        &found[3],
        if path.is_empty() { "/" } else { path }
    ))
}

#[tauri::command]
pub fn detect_run_targets(cwd: String) -> Vec<RunTarget> {
    detect(&cwd)
}

/// Whether the script `command` runs reads answers from the keyboard (a prompt library, readline): it is started with
/// a keyboard that answers it, instead of with none.
pub fn needs_keyboard(command: &str, cwd: &str) -> bool {
    script_of(command, cwd)
        .and_then(|script| wrapper_of(&script))
        .is_some_and(|file| uses_prompts(&Path::new(cwd).join(file)))
}

/// The log without colour codes (for the part of a log that is looked at as text).
pub fn plain(log: &str) -> String {
    strip_codes(log)
}

type ServerGate = Arc<tokio::sync::Mutex<()>>;
static SERVER_GATES: OnceLock<Mutex<HashMap<String, ServerGate>>> = OnceLock::new();

/// Serializes starts for one logical project server. This closes the gap between checking for an existing process
/// and registering a new one when two agents request the same app at nearly the same time.
async fn server_gate(command: &str, cwd: &str) -> tokio::sync::OwnedMutexGuard<()> {
    let gates = SERVER_GATES.get_or_init(|| Mutex::new(HashMap::new()));
    let gate = gates
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(crate::servers::identity(command, cwd))
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    gate.lock_owned().await
}

/// Starts `command` as a server AgentDeck owns and waits until a page answers. The address is the one the server
/// prints (a dev server moves to the next port when its own is taken); the guess `guess` is used when it prints none.
/// A server that already answers at the guess is not started again.
///
/// A script that asks questions is answered (`answers`, in order; else the first option, a free port, the default):
/// it starts with a keyboard when its wrapper reads one, and a script that turns out to wait for an answer is started
/// again with one. A guess whose port another program holds moves to the next free port.
///
/// Returns the page's address and, when this call started the server, the log file.
pub async fn bring_up(
    command: &str,
    cwd: &str,
    guess: Option<tauri::Url>,
    answers: &[String],
) -> Result<(tauri::Url, Option<std::path::PathBuf>), RunFailure> {
    let _gate = server_gate(command, cwd).await;
    let existing = crate::servers::find_running(command, cwd);
    if let Some(guess) = guess.as_ref() {
        if let Some(target) = crate::browser::ready(guess).await {
            return Ok((target, None));
        }
    }
    let mut guess = existing
        .as_ref()
        .and_then(|server| server.url.as_deref())
        .and_then(|url| crate::browser::parse(url).ok())
        .or(guess);
    let mut command = command.to_string();
    let requested_command = command.clone();
    let mut options = crate::servers::Options {
        interactive: needs_keyboard(&command, cwd),
        answers: answers.to_vec(),
        env: Vec::new(),
    };
    // Something else holds the port the page was expected on: the same server, on the next free one.
    if existing.is_none() {
        if let Some(port) = guess
            .as_ref()
            .and_then(|target| target.port_or_known_default())
        {
            let (moved, free, env) = crate::servers::prepare_port(&command, port);
            if free != port {
                command = moved;
                options.env = env;
                if let Some(target) = guess.as_mut() {
                    let _ = target.set_port(Some(free));
                }
            }
        }
    }
    // The log and the process are kept under the port; a command with no guess gets a number of its own.
    let key = existing.as_ref().map_or_else(
        || {
            guess
                .as_ref()
                .and_then(|target| target.port_or_known_default())
                .unwrap_or_else(|| {
                    use std::hash::{Hash, Hasher};
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    (&command, cwd).hash(&mut hasher);
                    40_000 + (hasher.finish() % 20_000) as u16
                })
        },
        |server| server.key,
    );
    let log = if existing.is_some() {
        crate::servers::log_path(key)?
    } else {
        crate::servers::start_with_identity(&command, cwd, key, &options, &requested_command, cwd)?
    };
    let mut started = std::time::Instant::now();
    // The log as it was at the last look, and since when it has not changed: a script waiting for an answer is silent.
    let mut seen = String::new();
    let mut quiet_since = started;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let text = crate::servers::read_log(key);
        if text != seen {
            seen = text;
            quiet_since = std::time::Instant::now();
        }
        // The address the server printed answers (over HTTP, at the scheme it speaks): that is the page.
        let printed = address_in(&seen)
            .as_deref()
            .and_then(|text| crate::browser::parse(text).ok());
        if let Some(printed) = printed {
            if let Some(target) = crate::browser::ready(&printed).await {
                crate::servers::set_url(key, target.as_str());
                return Ok((target, Some(log)));
            }
        }
        // It printed nothing we know: the guess, once it answers, is the page.
        if started.elapsed().as_secs() >= 6 {
            if let Some(target) = match guess.as_ref() {
                Some(guess) => crate::browser::ready(guess).await,
                None => None,
            } {
                crate::servers::set_url(key, target.as_str());
                return Ok((target, Some(log)));
            }
        }
        if crate::servers::exited(key) {
            return Err(failure(&command, cwd, &seen, Ended::Exited));
        }
        // It asks and nobody answers: it is started again with a keyboard, once. With one already, it is left to the user.
        if quiet_since.elapsed().as_secs() >= 5 && looks_interactive(&seen) {
            let _ = crate::servers::server_stop(Some(key));
            if options.interactive {
                return Err(failure(&command, cwd, &seen, Ended::Asking));
            }
            options.interactive = true;
            crate::servers::start_with_identity(
                &command,
                cwd,
                key,
                &options,
                &requested_command,
                cwd,
            )?;
            started = std::time::Instant::now();
            seen = String::new();
            quiet_since = started;
            continue;
        }
        if started.elapsed().as_secs() > 120 {
            return Err(failure(&command, cwd, &seen, Ended::Silent));
        }
    }
}

/// `bring_up` for the Preview's Run button: the address of the page, as text.
#[tauri::command]
pub async fn run_target(
    command: String,
    cwd: String,
    url: Option<String>,
) -> Result<String, RunFailure> {
    let guess = url.as_deref().map(crate::browser::parse).transpose()?;
    let (target, _) = bring_up(&command, &cwd, guess, &[]).await?;
    Ok(target.to_string())
}

/// Why a start failed, for the Preview to show: what happened, the last lines of the log (colour codes and noise
/// removed), and a hint when the log says what to do about it.
#[derive(Serialize, Debug, PartialEq)]
pub struct RunFailure {
    /// `exited`, `asking` (waits for an answer), `silent` (nothing came up in time), or `error`.
    pub kind: &'static str,
    /// One line.
    pub message: String,
    /// The last lines of the log.
    pub log: String,
    /// What to try, when it is known.
    pub hint: Option<String>,
    /// The script needs a keyboard: it belongs in the Terminal panel.
    pub interactive: bool,
    /// The script file the command runs, when it is a wrapper (`node scripts/dev.mjs`).
    pub wrapper: Option<String>,
}

impl From<String> for RunFailure {
    fn from(message: String) -> Self {
        RunFailure {
            kind: "error",
            message,
            log: String::new(),
            hint: None,
            interactive: false,
            wrapper: None,
        }
    }
}

impl RunFailure {
    /// The same as one text, for callers that have no layout (the agent's tools).
    pub fn text(&self) -> String {
        let mut text = self.message.clone();
        if let Some(hint) = &self.hint {
            text.push('\n');
            text.push_str(hint);
        }
        if !self.log.is_empty() {
            text.push_str("\nLast lines of its log:\n");
            text.push_str(&self.log);
        }
        text
    }
}

/// How a start ended without a page.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Ended {
    Exited,
    Asking,
    Silent,
}

/// The failure of `command` (run in `cwd`) given the log it left.
pub fn failure(command: &str, cwd: &str, raw_log: &str, ended: Ended) -> RunFailure {
    let wrapper = script_of(command, cwd).and_then(|script| wrapper_of(&script));
    let missing = missing_dependency(raw_log);
    // A process that stopped with an error is not waiting for anything; one that is silent after a question is.
    let asking = missing.is_none() && (ended == Ended::Asking || looks_interactive(raw_log));
    let (kind, message) = match (asking, ended) {
        (true, _) => ("asking", format!("`{command}` is waiting for an answer to a question, and AgentDeck runs servers with no keyboard.")),
        (false, Ended::Exited) => ("exited", format!("`{command}` stopped before a page came up.")),
        (false, _) => ("silent", format!("`{command}` did not start a page in two minutes.")),
    };
    let hint = if let Some(what) = missing {
        Some(format!(
            "Dependencies look missing ({what} was not found): run `{}` in {cwd}, then try again.",
            install_command(command)
        ))
    } else if asking {
        Some("Run it in the Terminal panel, answer its questions there, and open the address it prints in the Preview.".to_string())
    } else if raw_log.contains("EADDRINUSE")
        || raw_log.to_lowercase().contains("address already in use")
    {
        Some("The port is already taken by another program: stop it, or run the server on another port.".to_string())
    } else {
        wrapper.as_ref().map(|script| format!("`{script}` is a script of the project, not a dev server itself: its own output is below."))
    };
    RunFailure {
        kind,
        message,
        log: clean_log(raw_log, 30),
        hint,
        interactive: asking,
        wrapper,
    }
}

/// What the package.json script a command runs (`pnpm run dev`, `npm run dev`, `yarn dev`) contains.
fn script_of(command: &str, cwd: &str) -> Option<String> {
    let name = command.split_whitespace().last()?;
    let manifest = read_json(&Path::new(cwd).join("package.json"))?;
    manifest["scripts"][name].as_str().map(str::to_string)
}

/// The install command of the package manager a run command uses.
fn install_command(command: &str) -> &'static str {
    match command.split_whitespace().next() {
        Some("pnpm") => "pnpm install",
        Some("yarn") => "yarn install",
        Some("bun") => "bun install",
        _ => "npm install",
    }
}

/// What a log says could not be found when that is a package or a program (not a file of the project itself).
fn missing_dependency(log: &str) -> Option<String> {
    let log = strip_codes(log);
    if let Some(found) = Regex::new(r"(?i)cannot find (?:module|package) '([^']+)'")
        .ok()?
        .captures(&log)
    {
        let name = found[1].to_string();
        let own_file =
            name.starts_with(['.', '/', '\\']) || Regex::new(r"^[A-Za-z]:").ok()?.is_match(&name);
        if name.contains("node_modules") || !own_file {
            return Some(name);
        }
    }
    let program = r"[\w@./\\-]+";
    for pattern in [
        format!(r"(?i)'?({program})'? is not recognized as an internal or external command"),
        format!(r"(?i)(?:sh|bash|zsh)(?::\s*\d+)?:\s*({program}): (?:command )?not found"),
    ] {
        if let Some(found) = Regex::new(&pattern).ok()?.captures(&log) {
            return Some(found[1].to_string());
        }
    }
    None
}

/// Whether the end of a log looks like a question waiting for an answer (a prompt library, a menu, a yes/no).
pub fn looks_interactive(raw_log: &str) -> bool {
    let log = strip_codes(raw_log);
    // A prompt leaves the cursor on its line: the log does not end with a newline.
    let unfinished = !log.ends_with('\n') && log.trim_end().ends_with([':', '?', '\u{203a}']);
    let lines: Vec<&str> = log
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let last = &lines[lines.len().saturating_sub(5)..];
    unfinished || last.iter().any(|line| prompt_line(line))
}

fn prompt_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    [
        "use arrow keys",
        "arrow keys",
        "press enter",
        "[y/n]",
        "(y/n)",
        "(y/n/",
        "[y/n/",
        "(yes/no)",
    ]
    .iter()
    .any(|word| lower.contains(word))
        || line.ends_with('?')
        || line.starts_with("? ")
        || line.contains(" \u{203a} ")
        || line.ends_with('\u{203a}')
        || ["select ", "choose ", "which ", "pick "]
            .iter()
            .any(|word| lower.starts_with(word))
}

/// The log without colour codes and cursor movements (and without what a carriage return overwrote).
fn strip_codes(log: &str) -> String {
    let codes =
        Regex::new("\u{1b}\\[[0-9;?]*[A-Za-z]|\u{1b}\\][^\u{7}\u{1b}]*(?:\u{7}|\u{1b}\\\\)")
            .expect("a valid pattern");
    let plain = codes.replace_all(log, "");
    plain
        .split('\n')
        .map(|line| {
            line.rsplit('\r')
                .find(|part| !part.is_empty())
                .unwrap_or("")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The last `lines` lines of a log, readable: no colour codes, no package manager config warnings, no blank runs,
/// long lines cut, and at most about 3000 characters.
pub fn clean_log(raw_log: &str, lines: usize) -> String {
    let plain = strip_codes(raw_log);
    let mut kept: Vec<String> = Vec::new();
    for line in plain.lines() {
        let line = line.trim_end();
        if line.starts_with("npm warn Unknown ") || line.starts_with("npm WARN Unknown ") {
            continue;
        }
        if line.is_empty() && kept.last().is_none_or(|last| last.is_empty()) {
            continue;
        }
        kept.push(if line.chars().count() > 240 {
            format!("{}\u{2026}", line.chars().take(240).collect::<String>())
        } else {
            line.to_string()
        });
    }
    while kept.last().is_some_and(|last| last.is_empty()) {
        kept.pop();
    }
    let text = kept[kept.len().saturating_sub(lines)..].join("\n");
    if text.chars().count() > 3000 {
        let skip = text.chars().count() - 3000;
        format!("\u{2026}{}", text.chars().skip(skip).collect::<String>())
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_address_a_server_prints() {
        assert_eq!(address_in("  \u{1b}[32m➜\u{1b}[39m  Local:   \u{1b}[36mhttp://localhost:\u{1b}[1m5174\u{1b}[22m/\u{1b}[39m"), Some("http://localhost:5174/".into()), "colour codes inside the address");
        assert_eq!(
            address_in("  ➜  Local:   http://127.0.0.1:1420/"),
            Some("http://127.0.0.1:1420/".into()),
            "the host as printed"
        );
        assert_eq!(
            address_in("ready on http://0.0.0.0:3000"),
            Some("http://localhost:3000/".into())
        );
    }

    #[test]
    fn guesses_the_port_from_the_script() {
        let none = Path::new("/nonexistent");
        assert_eq!(
            port_of("vite --host 127.0.0.1 --port 1420", "", none),
            Some(1420)
        );
        assert_eq!(port_of("next dev", "", none), Some(3000));
        assert_eq!(port_of("ng serve", "", none), Some(4200));
        assert_eq!(port_of("vite", "", none), Some(5173));
        assert_eq!(port_of("node server.js", "", none), None);
    }

    #[test]
    fn keeps_the_scheme_a_server_prints() {
        // What `nx serve` prints for an app served with a certificate: the first address is the page, https only.
        let log = "  \u{1b}[32m\u{279c}\u{1b}[39m  Local:   https://localhost:4205/\n  \u{279c}  Local:   https://vite.localhost:4205/\n  \u{279c}  Network: https://192.168.1.6:4205/";
        assert_eq!(address_in(log), Some("https://localhost:4205/".into()));
    }

    #[test]
    fn finds_a_wrapper_script() {
        assert_eq!(
            wrapper_of("node scripts/z-dev-vite.mjs"),
            Some("scripts/z-dev-vite.mjs".into())
        );
        assert_eq!(
            wrapper_of(
                "cross-env NODE_ENV=development node --no-warnings scripts/dev.js --app web"
            ),
            Some("scripts/dev.js".into())
        );
        assert_eq!(wrapper_of("vite --port 5173"), None);
        assert_eq!(wrapper_of("nx serve his-medic"), None);
    }

    const MISSING_NX: &str = "npm warn Unknown env config \"global-bin-dir\". This will stop working in the next major version of npm.\n\n> app@0.0.0 dev E:\\proj\n> node scripts/z-dev-vite.mjs\n\nnode:internal/modules/cjs/loader:1386\n  throw err;\n  ^\n\nError: Cannot find module 'E:\\proj\\node_modules\\nx\\bin\\nx.js'\n    at Function._resolveFilename (node:internal/modules/cjs/loader:1383:15) {\n  code: 'MODULE_NOT_FOUND',\n  requireStack: []\n}\n\nNode.js v24.0.0\n";

    #[test]
    fn missing_packages_are_told_apart_from_missing_files() {
        assert!(missing_dependency(MISSING_NX).is_some_and(|what| what.contains("nx.js")));
        assert_eq!(
            missing_dependency("Error: Cannot find module 'vite'"),
            Some("vite".into())
        );
        assert_eq!(
            missing_dependency("Error: Cannot find module './config.js'"),
            None
        );
        assert_eq!(
            missing_dependency("'nx' is not recognized as an internal or external command,"),
            Some("nx".into())
        );
        assert_eq!(
            missing_dependency("sh: 1: vite: not found"),
            Some("vite".into())
        );
        assert_eq!(missing_dependency("Compiled successfully"), None);
    }

    #[test]
    fn a_failed_wrapper_is_explained_with_the_log_cleaned() {
        let folder = std::env::temp_dir().join("agentdeck-detect-test-wrapper");
        std::fs::create_dir_all(&folder).expect("folder");
        std::fs::write(
            folder.join("package.json"),
            r#"{"scripts":{"dev":"node scripts/z-dev-vite.mjs"}}"#,
        )
        .expect("manifest");
        let failed = failure(
            "pnpm run dev",
            &folder.to_string_lossy(),
            MISSING_NX,
            Ended::Exited,
        );
        assert_eq!(failed.kind, "exited");
        assert_eq!(failed.wrapper.as_deref(), Some("scripts/z-dev-vite.mjs"));
        assert!(!failed.interactive);
        let hint = failed.hint.as_deref().unwrap_or_default();
        assert!(
            hint.contains("pnpm install") && hint.contains("missing"),
            "{hint}"
        );
        assert!(
            failed.log.contains("MODULE_NOT_FOUND") && !failed.log.contains("npm warn Unknown"),
            "{}",
            failed.log
        );
        assert!(failed.text().contains("Last lines of its log:"));
        let _ = std::fs::remove_dir_all(folder);
    }

    #[test]
    fn a_question_is_recognised_and_a_build_is_not() {
        assert!(looks_interactive(
            "\u{1b}[36m?\u{1b}[39m Which app do you want to run? \u{203a} \n"
        ));
        assert!(looks_interactive(
            "Select the app to start (Use arrow keys)\n\u{276f} his-medic\n  emr-portal"
        ));
        assert!(looks_interactive("Continue? [y/N]"));
        assert!(looks_interactive("Choose a step: "));
        assert!(!looks_interactive("\u{276f} Building...\n\u{2714} Building...\nWatch mode enabled. Watching for file changes...\n"));
        assert!(!looks_interactive(
            "  \u{279c}  Local:   https://localhost:4205/\n"
        ));
        let asking = failure(
            "npm run dev",
            "/nonexistent",
            "Which app do you want to run? \u{203a} ",
            Ended::Asking,
        );
        assert_eq!(asking.kind, "asking");
        assert!(
            asking.interactive
                && asking
                    .hint
                    .as_deref()
                    .is_some_and(|hint| hint.contains("Terminal"))
        );
    }

    #[test]
    fn the_log_is_cut_to_something_readable() {
        let long = "x".repeat(500);
        let raw = format!("a\r\nprogress 10%\rprogress 100%\n\n\n\n{long}\n");
        let cleaned = clean_log(&raw, 30);
        assert_eq!(cleaned.lines().count(), 4, "{cleaned}");
        assert!(cleaned.contains("progress 100%") && !cleaned.contains("progress 10%"));
        assert!(cleaned
            .lines()
            .last()
            .is_some_and(|line| line.chars().count() <= 241));
        let many: String = (0..100).map(|n| format!("line {n}\n")).collect();
        assert_eq!(clean_log(&many, 5).lines().count(), 5);
    }

    #[test]
    fn a_package_manager_is_chosen_from_the_command() {
        assert_eq!(install_command("pnpm run dev"), "pnpm install");
        assert_eq!(install_command("yarn dev"), "yarn install");
        assert_eq!(install_command("npm run start"), "npm install");
    }
}
