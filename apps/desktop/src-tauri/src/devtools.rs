//! Tools of the `agentdeck` MCP server that help an agent work on a project the way the project expects:
//!
//! - `project_context`: the stack, the scripts, the rules files and the design tokens of the project, so UI work uses
//!   what is there (its tokens, its components) instead of inventing a look.
//! - `run_checks`: runs the project's own lint / type check / tests / build (only the scripts it declares, never an
//!   arbitrary command) and answers the problems as `file:line` lines, then the end of the output.
//! - `server_logs`: the end of the logs of the dev servers AgentDeck started with `browser_serve`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use regex::Regex;
use serde_json::{json, Value};

const TOOLS: [&str; 3] = ["project_context", "run_checks", "server_logs"];

pub fn is_tool(name: &str) -> bool {
    TOOLS.contains(&name)
}

pub fn tool_defs() -> Vec<Value> {
    let root = json!({ "type": "string", "description": "absolute path of the project folder; left out: the chat's project" });
    vec![
        json!({
            "name": "project_context",
            "description": "What this project is made of and how it wants to be worked on: frameworks and UI libraries, package scripts (lint, check, test, build), the rules files (AGENTS.md, CLAUDE.md, design notes) and the design tokens (CSS custom properties such as --color-primary, --radius). Call it before building or changing UI, and use those tokens and components instead of new colours, sizes or hand-made controls.",
            "inputSchema": { "type": "object", "properties": { "root": root }, "required": [] },
        }),
        json!({
            "name": "run_checks",
            "description": "Runs the project's own checks and answers the problems as file:line lines (then the end of the output). Only scripts the project declares run (package.json lint / typecheck / check / test / build, or cargo for a Rust workspace), never an arbitrary command. Run it after a change and before saying the work is done; fix what it reports.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "description": "lint, typecheck, test, build, or all (lint + typecheck); default all" },
                    "root": root,
                },
                "required": [],
            },
        }),
        json!({
            "name": "server_logs",
            "description": "The end of the output of the dev servers AgentDeck runs (started with browser_serve): compile errors, stack traces, request logs. Read it when the page is blank, shows an error overlay, or a request fails.",
            "inputSchema": {
                "type": "object",
                "properties": { "lines": { "type": "number", "description": "lines per server from the end (default 80, at most 400)" } },
                "required": [],
            },
        }),
    ]
}

pub async fn call(name: &str, args: &Value, chat: &str) -> Result<String, String> {
    match name {
        "server_logs" => Ok(server_logs(args["lines"].as_u64().unwrap_or(80).clamp(5, 400) as usize)),
        _ => {
            let root = PathBuf::from(crate::index::root_of(chat, args)?);
            if !root.is_dir() {
                return Err(format!("{} is not a folder", root.display()));
            }
            if name == "project_context" {
                Ok(project_context(&root))
            } else {
                run_checks(&root, args["kind"].as_str().unwrap_or("all")).await
            }
        }
    }
}

// --- project_context --------------------------------------------------------------------------------------------

fn read(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{cut}\n… (cut at {max} characters)")
}

/// The package.json files of the project: its root, then one level of workspace folders (apps/*, packages/*).
fn manifests(root: &Path) -> Vec<(PathBuf, Value)> {
    let mut found = Vec::new();
    let mut look = |folder: &Path| {
        if let Some(json) = read(&folder.join("package.json")).and_then(|text| serde_json::from_str::<Value>(&text).ok()) {
            found.push((folder.to_path_buf(), json));
        }
    };
    look(root);
    for group in ["apps", "packages", "libs"] {
        if let Ok(entries) = std::fs::read_dir(root.join(group)) {
            for entry in entries.flatten().take(20) {
                if entry.path().is_dir() {
                    look(&entry.path());
                }
            }
        }
    }
    found
}

const UI_PACKAGES: &[&str] = &[
    "svelte", "@sveltejs/kit", "react", "next", "vue", "nuxt", "@angular/core", "solid-js", "astro", "tailwindcss",
    "@tailwindcss/vite", "bootstrap", "@mui/material", "antd", "@chakra-ui/react", "@radix-ui/react-dialog",
    "shadcn-ui", "bits-ui", "@shival99/z-ui-svelte", "@shival99/z-ui", "lucide-svelte", "lucide-react",
    "@tauri-apps/api", "electron", "vite",
];

fn project_context(root: &Path) -> String {
    let mut out = vec![format!("Project: {}", root.display())];

    // Stack and scripts.
    let packages = manifests(root);
    let manager = package_manager(root);
    if !packages.is_empty() {
        out.push(format!("\n## Packages (run scripts with `{manager} run <script>` in their folder)"));
    }
    for (folder, json) in &packages {
        let rel = folder.strip_prefix(root).map_or_else(|_| folder.display().to_string(), |path| {
            let text = path.display().to_string();
            if text.is_empty() { ".".to_string() } else { text }
        });
        let mut deps: Vec<&str> = ["dependencies", "devDependencies"]
            .iter()
            .filter_map(|key| json[*key].as_object())
            .flat_map(|map| map.keys().map(String::as_str))
            .filter(|name| UI_PACKAGES.contains(name))
            .collect();
        deps.sort_unstable();
        deps.dedup();
        let scripts: Vec<String> = json["scripts"]
            .as_object()
            .map(|map| map.iter().take(25).map(|(name, command)| format!("  - {name}: {}", command.as_str().unwrap_or_default())).collect())
            .unwrap_or_default();
        out.push(format!(
            "- {rel} ({}){}",
            json["name"].as_str().unwrap_or("unnamed"),
            if deps.is_empty() { String::new() } else { format!(" uses {}", deps.join(", ")) }
        ));
        out.extend(scripts);
    }
    if root.join("Cargo.toml").is_file() {
        out.push("- Rust workspace (Cargo.toml): cargo check / clippy / test".to_string());
    }

    // Rules the project wrote for whoever works on it.
    let rules = ["AGENTS.md", "CLAUDE.md", ".cursorrules", "DESIGN.md", "docs/DESIGN.md", "CONTRIBUTING.md"];
    for name in rules {
        if let Some(text) = read(&root.join(name)) {
            out.push(format!("\n## {name}\n{}", clip(text.trim(), 4000)));
        }
    }

    // Design tokens: custom properties defined in the project's stylesheets.
    let tokens = design_tokens(root);
    if !tokens.is_empty() {
        out.push(format!(
            "\n## Design tokens ({} found; use these with var(--name) or the matching utility classes, do not hard-code colours or sizes)",
            tokens.len()
        ));
        out.extend(tokens.into_iter().take(160).map(|(name, value, file)| format!("- --{name}: {value}  ({file})")));
    }
    out.push("\nBefore saying UI work is done: open it in the Preview, run ui_audit, and run run_checks.".to_string());
    out.join("\n")
}

/// `--name: value` pairs from the CSS of the project (not node_modules or build output), the first definition kept.
fn design_tokens(root: &Path) -> Vec<(String, String, String)> {
    let Ok(pattern) = Regex::new(r"(?m)^\s*--([A-Za-z0-9_-]+)\s*:\s*([^;{}]+);") else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut tokens = Vec::new();
    let walker = ignore::WalkBuilder::new(root).max_depth(Some(6)).build();
    for entry in walker.flatten().take(4000) {
        let path = entry.path();
        let css = path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| matches!(ext, "css" | "scss" | "pcss" | "postcss"));
        let text_path = path.to_string_lossy();
        if !css || text_path.contains("node_modules") || text_path.contains("dist") || text_path.contains("target") {
            continue;
        }
        let Some(text) = read(path) else { continue };
        let rel = path.strip_prefix(root).map_or_else(|_| text_path.to_string(), |rel| rel.display().to_string());
        for found in pattern.captures_iter(&text) {
            let name = found[1].to_string();
            if seen.insert(name.clone()) {
                tokens.push((name, found[2].trim().chars().take(80).collect(), rel.clone()));
            }
        }
        if tokens.len() > 400 {
            break;
        }
    }
    tokens
}

// --- run_checks --------------------------------------------------------------------------------------------------

fn package_manager(root: &Path) -> &'static str {
    let mut folder = Some(root);
    while let Some(dir) = folder {
        if dir.join("pnpm-lock.yaml").is_file() {
            return "pnpm";
        }
        if dir.join("yarn.lock").is_file() {
            return "yarn";
        }
        if dir.join("bun.lockb").is_file() || dir.join("bun.lock").is_file() {
            return "bun";
        }
        if dir.join("package-lock.json").is_file() {
            return "npm";
        }
        folder = dir.parent();
    }
    "npm"
}

/// Script names that do each kind of check, in order of preference.
fn script_names(kind: &str) -> &'static [&'static str] {
    match kind {
        "lint" => &["lint"],
        "typecheck" => &["typecheck", "type-check", "check", "tsc", "check-types"],
        "test" => &["test"],
        "build" => &["build"],
        _ => &[],
    }
}

/// The commands a check of `kind` runs: (folder, program, args, label). Only what the project declares.
fn plan(root: &Path, kind: &str) -> Vec<(PathBuf, String, Vec<String>, String)> {
    let kinds: Vec<&str> = if kind == "all" { vec!["lint", "typecheck"] } else { vec![kind] };
    let manager = package_manager(root);
    let mut steps = Vec::new();
    for kind in &kinds {
        for (folder, json) in manifests(root).into_iter().take(6) {
            if let Some(script) = script_names(kind).iter().find(|name| json["scripts"][**name].is_string()) {
                let rel = folder.strip_prefix(root).map(|path| path.display().to_string()).unwrap_or_default();
                let label = format!("{manager} run {script}{}", if rel.is_empty() { String::new() } else { format!(" (in {rel})") });
                steps.push((folder, manager.to_string(), vec!["run".to_string(), (*script).to_string()], label));
            }
        }
        if root.join("Cargo.toml").is_file() {
            let cargo = match *kind {
                "lint" => Some(vec!["clippy", "--workspace", "--message-format=short"]),
                "typecheck" => Some(vec!["check", "--workspace", "--message-format=short"]),
                "test" => Some(vec!["test", "--workspace"]),
                "build" => Some(vec!["build", "--workspace", "--message-format=short"]),
                _ => None,
            };
            if let Some(args) = cargo {
                let label = format!("cargo {}", args.join(" "));
                steps.push((root.to_path_buf(), "cargo".to_string(), args.into_iter().map(str::to_string).collect(), label));
            }
        }
    }
    steps
}

/// Lines that name a place in a file (`path:line[:col]`, `path(line,col)`, `--> path:line:col`), deduplicated.
fn diagnostics(output: &str) -> Vec<String> {
    let Ok(place) = Regex::new(r"(?i)(?:-->\s*)?([A-Za-z]:[\\/][^\s:()]+|[\w./\\-]+)\.(ts|tsx|js|jsx|mjs|cjs|svelte|vue|astro|rs|py|go|css|scss|html)(?::(\d+)(?::(\d+))?|\((\d+),(\d+)\))") else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    let mut found = Vec::new();
    for line in output.lines() {
        let clean = strip_ansi(line);
        if !place.is_match(&clean) || !(clean.to_lowercase().contains("error") || clean.to_lowercase().contains("warn") || clean.contains("-->") || clean.contains('✖') || clean.contains('×')) {
            continue;
        }
        let text = clean.trim().chars().take(300).collect::<String>();
        if seen.insert(text.clone()) {
            found.push(text);
        }
        if found.len() >= 60 {
            break;
        }
    }
    found
}

fn strip_ansi(text: &str) -> String {
    Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").map_or_else(|_| text.to_string(), |ansi| ansi.replace_all(text, "").into_owned())
}

async fn run_checks(root: &Path, kind: &str) -> Result<String, String> {
    if !matches!(kind, "all" | "lint" | "typecheck" | "test" | "build") {
        return Err("kind must be lint, typecheck, test, build or all".to_string());
    }
    let steps = plan(root, kind);
    if steps.is_empty() {
        return Ok(format!("This project declares no {kind} script (package.json scripts lint / typecheck / check / test / build, or a Cargo workspace). Nothing was run."));
    }
    let mut report = Vec::new();
    let mut failed = 0;
    for (folder, program, args, label) in steps {
        let mut command = tokio::process::Command::new(if cfg!(windows) { "cmd" } else { program.as_str() });
        if cfg!(windows) {
            // Package managers are `.cmd` shims on Windows: they run through cmd.exe.
            command.arg("/C").arg(&program);
        }
        command.args(&args).current_dir(&folder).kill_on_drop(true).env("FORCE_COLOR", "0").env("CI", "1");
        #[cfg(windows)]
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let started = std::time::Instant::now();
        let output = match tokio::time::timeout(Duration::from_secs(600), command.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                failed += 1;
                report.push(format!("## {label}: could not start ({error})"));
                continue;
            }
            Err(_) => {
                failed += 1;
                report.push(format!("## {label}: stopped after 10 minutes"));
                continue;
            }
        };
        let text = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let ok = output.status.success();
        if !ok {
            failed += 1;
        }
        let problems = diagnostics(&text);
        let tail: Vec<String> = strip_ansi(&text).lines().filter(|line| !line.trim().is_empty()).map(str::to_string).collect();
        let tail = tail[tail.len().saturating_sub(if ok { 8 } else { 40 })..].join("\n");
        report.push(format!(
            "## {label}: {} in {:.0}s{}\n{}{}",
            if ok { "passed" } else { "FAILED" },
            started.elapsed().as_secs_f64(),
            if problems.is_empty() { String::new() } else { format!(", {} problem lines", problems.len()) },
            if problems.is_empty() { String::new() } else { format!("{}\n\nEnd of the output:\n", problems.join("\n")) },
            tail
        ));
    }
    let head = if failed == 0 { "All checks passed.".to_string() } else { format!("{failed} check(s) failed: fix the problems below, then run_checks again.") };
    Ok(format!("{head}\n\n{}", report.join("\n\n")))
}

// --- server_logs -------------------------------------------------------------------------------------------------

fn server_logs(lines: usize) -> String {
    let servers = crate::servers::servers_list();
    if servers.is_empty() {
        return "AgentDeck runs no dev server now. Start one with browser_serve; a server started in your own shell is not tracked here.".to_string();
    }
    servers
        .iter()
        .map(|server| {
            let log = strip_ansi(&crate::servers::read_log(server.key));
            let all: Vec<&str> = log.lines().collect();
            let tail = all[all.len().saturating_sub(lines)..].join("\n");
            format!(
                "## {} ({}) in {}: {}{}\n{}",
                server.command,
                server.url.as_deref().unwrap_or("no address yet"),
                server.cwd,
                if server.running { "running" } else { "EXITED" },
                format_args!(", {}s old", server.age),
                if tail.trim().is_empty() { "(no output yet)" } else { tail.as_str() }
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_problem_lines_of_common_tools() {
        let output = "\
src/lib/a.ts:12:5 - error TS2322: Type 'string' is not assignable\n\
/home/x/app/src/B.svelte:40:3\nError: unknown prop\n\
warning: unused variable `x`\n  --> src/main.rs:7:9\n\
  12:3  error  'foo' is not defined  no-undef\n\
all good line\n";
        let found = diagnostics(output);
        assert!(found.iter().any(|line| line.contains("src/lib/a.ts:12:5")));
        assert!(found.iter().any(|line| line.contains("src/main.rs:7:9")));
        assert!(!found.iter().any(|line| line.contains("all good")));
    }

    #[test]
    fn picks_declared_scripts_only() {
        assert_eq!(script_names("typecheck")[0], "typecheck");
        assert!(script_names("rm -rf").is_empty());
    }
}
