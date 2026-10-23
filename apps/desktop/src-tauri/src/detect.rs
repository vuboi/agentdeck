//! How a project can be run, read from its own files, so the Preview can offer a button instead of a typed prompt.
//!
//! Looked at: `package.json` scripts (the project folder, its sub-folders, and `apps/*`, `packages/*`, `services/*`),
//! `.claude/launch.json`, and Django's `manage.py`. The address a server will answer at is only a guess made from the
//! script (its `--port`, its framework's usual port); `run` reads the real one from the server's own output.

use std::path::{Path, PathBuf};

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
}

const SKIP_DIRS: [&str; 8] = ["node_modules", "target", "dist", "build", "out", "coverage", "venv", "__pycache__"];
const SKIP_WORDS: [&str; 10] = ["build", "test", "lint", "deploy", "format", "check", "clean", "publish", "release", "e2e"];
/// Script names that start something worth looking at, best first.
const RANK: [&str; 8] = ["dev", "start", "serve", "preview", "web", "storybook", "docs", "watch"];
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
            .map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()).collect())
            .unwrap_or_default();
        list.retain(|path| {
            let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
            path.is_dir() && !name.starts_with('.') && !SKIP_DIRS.contains(&name)
        });
        list.sort();
        list
    };
    for child in children(root) {
        let name = child.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
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
        for (file, program) in [("pnpm-lock.yaml", "pnpm run"), ("yarn.lock", "yarn"), ("bun.lockb", "bun run"), ("bun.lock", "bun run"), ("package-lock.json", "npm run")] {
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
    let Some(manifest) = read_json(&folder.join("package.json")) else { return };
    let Some(scripts) = manifest["scripts"].as_object() else { return };
    let dependencies = format!("{} {}", manifest["dependencies"], manifest["devDependencies"]);
    let package = manifest["name"].as_str().map(str::to_string).unwrap_or_else(|| folder.file_name().and_then(|name| name.to_str()).unwrap_or("app").to_string());
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
        (RANK.iter().position(|rank| lower == *rank).unwrap_or(RANK.len()), lower)
    });
    for name in names {
        let text = scripts[name].as_str().unwrap_or_default();
        // The desktop app and the packaged ones are not a page.
        if ["tauri", "electron", "expo", "react-native", "nodemon"].iter().any(|word| text.contains(word)) {
            continue;
        }
        let port = port_of(text, &dependencies, folder);
        found.push(RunTarget {
            name: if folder == root { name.clone() } else { format!("{package}: {name}") },
            command: runner(root, folder, name),
            cwd: folder.to_string_lossy().into_owned(),
            url: port.map(|port| format!("http://localhost:{port}/")),
            source: "package.json".into(),
        });
    }
}

/// The port a script asks for, else the usual one of the framework it runs.
fn port_of(script: &str, dependencies: &str, folder: &Path) -> Option<u16> {
    let explicit = Regex::new(r"(?:--port[= ]|-p[= ]|PORT=|:)(\d{4,5})\b").ok()?;
    if let Some(port) = explicit.captures(script).and_then(|found| found[1].parse().ok()) {
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
    .find(|(word, _)| script.contains(word) || (!word.contains(' ') && dependencies.contains(&format!("\"{word}\""))))
    .map(|(_, port)| *port)
}

/// `server: { port: N }` of a vite config, when it has one.
fn config_port(folder: &Path) -> Option<u16> {
    let pattern = Regex::new(r"\bport\s*:\s*(\d{4,5})").ok()?;
    ["vite.config.ts", "vite.config.js", "vite.config.mts", "vite.config.mjs", "svelte.config.js"]
        .iter()
        .filter_map(|name| std::fs::read_to_string(folder.join(name)).ok())
        .find_map(|text| pattern.captures(&text).and_then(|found| found[1].parse().ok()))
}

/// The run configurations the project already wrote down (`.claude/launch.json`).
fn launch_json(root: &Path, found: &mut Vec<RunTarget>) {
    let Some(file) = read_json(&root.join(".claude").join("launch.json")) else { return };
    for item in file["configurations"].as_array().into_iter().flatten() {
        let Some(program) = item["runtimeExecutable"].as_str() else { continue };
        let args: Vec<&str> = item["runtimeArgs"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let port = item["port"].as_u64();
        found.push(RunTarget {
            name: item["name"].as_str().unwrap_or(program).to_string(),
            command: std::iter::once(program).chain(args).collect::<Vec<_>>().join(" "),
            cwd: item["cwd"].as_str().map_or_else(|| root.to_string_lossy().into_owned(), |cwd| root.join(cwd).to_string_lossy().into_owned()),
            url: item["url"].as_str().map(str::to_string).or_else(|| port.map(|port| format!("http://localhost:{port}/"))),
            source: "launch.json".into(),
        });
    }
}

fn django(root: &Path, found: &mut Vec<RunTarget>) {
    if root.join("manage.py").is_file() {
        found.push(RunTarget {
            name: "Django".into(),
            command: if cfg!(windows) { "python manage.py runserver" } else { "python3 manage.py runserver" }.into(),
            cwd: root.to_string_lossy().into_owned(),
            url: Some("http://localhost:8000/".into()),
            source: "manage.py".into(),
        });
    }
}

/// The first local address a server prints about itself (`Local: http://localhost:5173/`), colour codes removed.
pub fn address_in(log: &str) -> Option<String> {
    let colours = Regex::new("\u{1b}\\[[0-9;]*m").ok()?;
    let plain = colours.replace_all(log, "");
    let found = Regex::new(r"https?://(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1?\]):(\d{2,5})[^\s)\]]*").ok()?.captures(&plain)?;
    Some(format!("http://localhost:{}/", &found[1]))
}

#[tauri::command]
pub fn detect_run_targets(cwd: String) -> Vec<RunTarget> {
    detect(&cwd)
}

/// Starts `command` as a server AgentDeck owns and returns the address it answers at, once it does. The address is
/// the one the server prints (a dev server moves to the next port when its own is taken); the guess `url` is used
/// when it prints none. A server that already answers at the guess is not started again.
#[tauri::command]
pub async fn run_target(command: String, cwd: String, url: Option<String>) -> Result<String, String> {
    let guess = url.as_deref().map(crate::browser::parse).transpose()?;
    if let Some(target) = guess.as_ref().filter(|target| crate::browser::answers(target)) {
        return Ok(target.to_string());
    }
    // The log and the process are kept under the port; a command with no guess gets a number of its own.
    let key = guess.as_ref().and_then(|target| target.port_or_known_default()).unwrap_or_else(|| {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (&command, &cwd).hash(&mut hasher);
        40_000 + (hasher.finish() % 20_000) as u16
    });
    crate::servers::start(&command, &cwd, key)?;
    let started = std::time::Instant::now();
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if let Some(target) = address_in(&crate::servers::tail(key, 200)).as_deref().and_then(|text| crate::browser::parse(text).ok()) {
            if crate::browser::answers(&target) {
                crate::servers::set_url(key, target.as_str());
                return Ok(target.to_string());
            }
        }
        // It printed nothing we know: the guess, once it answers, is the page.
        if let Some(target) = guess.as_ref().filter(|target| started.elapsed().as_secs() >= 6 && crate::browser::answers(target)) {
            crate::servers::set_url(key, target.as_str());
            return Ok(target.to_string());
        }
        if crate::servers::exited(key) || started.elapsed().as_secs() > 120 {
            return Err(format!("`{command}` did not start a page. Last lines of its log:\n{}", crate::servers::tail(key, 30)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_address_a_server_prints() {
        assert_eq!(address_in("  \u{1b}[32m➜\u{1b}[39m  Local:   \u{1b}[36mhttp://localhost:\u{1b}[1m5174\u{1b}[22m/\u{1b}[39m"), Some("http://localhost:5174/".into()), "colour codes inside the address");
        assert_eq!(address_in("  ➜  Local:   http://127.0.0.1:1420/"), Some("http://localhost:1420/".into()));
        assert_eq!(address_in("ready on http://0.0.0.0:3000"), Some("http://localhost:3000/".into()));
    }

    #[test]
    fn guesses_the_port_from_the_script() {
        let none = Path::new("/nonexistent");
        assert_eq!(port_of("vite --host 127.0.0.1 --port 1420", "", none), Some(1420));
        assert_eq!(port_of("next dev", "", none), Some(3000));
        assert_eq!(port_of("ng serve", "", none), Some(4200));
        assert_eq!(port_of("vite", "", none), Some(5173));
        assert_eq!(port_of("node server.js", "", none), None);
    }
}
