//! The `/` palette of the message box: the commands and skills each agent already has on this machine.
//!
//! Read-only. Claude Code keeps custom commands in `~/.claude/commands` and `<project>/.claude/commands` (and in
//! plugins), Codex keeps prompts in `~/.codex/prompts`, and all three agents read skills (`<name>/SKILL.md`) from
//! their own folders. A command is a Markdown file; the text it holds is what AgentDeck sends, so a command works
//! the same whichever agent the chat is on. Only files inside those folders can be read back.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct ProviderCommand {
    /// What is typed after the slash. Nested folders and plugins are joined with `:`.
    pub name: String,
    pub description: String,
    /// `prompt` (a command file) or `skill` (a `SKILL.md`).
    pub kind: &'static str,
    pub path: String,
    /// Where it was found: `user`, `project` or the name of a plugin.
    pub origin: String,
}

/// `---` front matter of a Markdown file as `key: value` lines, and the text after it.
fn split_front_matter(text: &str) -> (BTreeMap<String, String>, &str) {
    let mut fields = BTreeMap::new();
    let trimmed = text.trim_start_matches('\u{feff}');
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (fields, trimmed);
    };
    let rest = rest.trim_start_matches(['\r', '\n']);
    let Some(end) = rest.find("\n---") else {
        return (fields, trimmed);
    };
    let mut lines = rest[..end].lines().peekable();
    while let Some(line) = lines.next() {
        if let Some((key, value)) = line.split_once(':') {
            let mut value = value.trim().trim_matches(['"', '\'']).to_owned();
            // A YAML block (`description: >` or `|`, then indented lines): the value is those lines, not the marker.
            if matches!(value.trim_end_matches(['-', '+']), ">" | "|") {
                let mut parts: Vec<&str> = Vec::new();
                while let Some(next) = lines.next_if(|next| next.is_empty() || next.starts_with([' ', '\t'])) {
                    if !next.trim().is_empty() {
                        parts.push(next.trim());
                    }
                }
                value = parts.join(" ");
            }
            if !key.starts_with(' ') && !value.is_empty() {
                fields.insert(key.trim().to_owned(), value);
            }
        }
    }
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']);
    (fields, body)
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(|line| line.trim().trim_start_matches('#').trim())
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .chars()
        .take(140)
        .collect()
}

fn describe(fields: &BTreeMap<String, String>, body: &str) -> String {
    fields
        .get("description")
        .cloned()
        .unwrap_or_else(|| first_line(body))
        .chars()
        .take(160)
        .collect()
}

/// The command files of a folder, with the folders under it as `folder:name` (three levels at most).
fn commands_in(
    dir: &Path,
    origin: &str,
    prefix: &str,
    depth: usize,
    out: &mut Vec<ProviderCommand>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if depth < 3 {
                commands_in(&path, origin, &format!("{prefix}{name}:"), depth + 1, out);
            }
            continue;
        }
        let Some(stem) = name.strip_suffix(".md") else {
            continue;
        };
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let (fields, body) = split_front_matter(&text);
        out.push(ProviderCommand {
            name: format!("{prefix}{stem}"),
            description: describe(&fields, body),
            kind: "prompt",
            path: path.to_string_lossy().into_owned(),
            origin: origin.to_owned(),
        });
    }
}

/// The skills of a folder: every sub-folder that holds a `SKILL.md`.
fn skills_in(dir: &Path, origin: &str, prefix: &str, out: &mut Vec<ProviderCommand>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file = entry.path().join("SKILL.md");
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let (fields, body) = split_front_matter(&text);
        let folder = entry
            .file_name()
            .to_string_lossy()
            .trim_end_matches('@')
            .to_owned();
        let name = fields.get("name").cloned().unwrap_or(folder);
        out.push(ProviderCommand {
            name: format!("{prefix}{name}"),
            description: describe(&fields, body),
            kind: "skill",
            path: file.to_string_lossy().into_owned(),
            origin: origin.to_owned(),
        });
    }
}

/// Plugin folders of Claude Code: `plugins/cache/<marketplace>/<plugin>/<version>/{commands,skills}`.
fn claude_plugins(home: &Path, out: &mut Vec<ProviderCommand>) {
    let cache = home.join(".claude").join("plugins").join("cache");
    let dirs = |path: &Path| -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        found.sort();
        found
    };
    for market in dirs(&cache) {
        for plugin in dirs(&market) {
            let name = plugin
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            // The newest version is the last in name order.
            if let Some(version) = dirs(&plugin).pop() {
                commands_in(
                    &version.join("commands"),
                    &name,
                    &format!("{name}:"),
                    0,
                    out,
                );
                skills_in(&version.join("skills"), &name, &format!("{name}:"), out);
            }
        }
    }
}

/// Everything the agent offers after a `/`, project folders first (they win over user ones of the same name).
pub fn list(agent: &str, home: &Path, project: Option<&Path>) -> Vec<ProviderCommand> {
    let mut found = Vec::new();
    match agent {
        "claude" => {
            if let Some(project) = project {
                commands_in(
                    &project.join(".claude").join("commands"),
                    "project",
                    "",
                    0,
                    &mut found,
                );
                skills_in(
                    &project.join(".claude").join("skills"),
                    "project",
                    "",
                    &mut found,
                );
            }
            commands_in(
                &home.join(".claude").join("commands"),
                "user",
                "",
                0,
                &mut found,
            );
            skills_in(&home.join(".claude").join("skills"), "user", "", &mut found);
            claude_plugins(home, &mut found);
        }
        "codex" => {
            if let Some(project) = project {
                skills_in(
                    &project.join(".codex").join("skills"),
                    "project",
                    "",
                    &mut found,
                );
                skills_in(
                    &project.join(".agents").join("skills"),
                    "project",
                    "",
                    &mut found,
                );
            }
            commands_in(
                &home.join(".codex").join("prompts"),
                "user",
                "",
                0,
                &mut found,
            );
            skills_in(&home.join(".codex").join("skills"), "user", "", &mut found);
            skills_in(&home.join(".agents").join("skills"), "user", "", &mut found);
        }
        "antigravity" => {
            if let Some(project) = project {
                skills_in(
                    &project.join(".agents").join("skills"),
                    "project",
                    "",
                    &mut found,
                );
            }
            skills_in(
                &home.join(".gemini").join("config").join("skills"),
                "user",
                "",
                &mut found,
            );
            skills_in(&home.join(".agents").join("skills"), "user", "", &mut found);
        }
        _ => {}
    }
    let mut seen = std::collections::HashSet::new();
    found.retain(|item| seen.insert((item.name.clone(), item.kind)));
    found
}

/// The folders a command file may be read from.
fn allowed_roots(home: &Path, project: Option<&Path>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = [".claude", ".codex", ".gemini", ".agents"]
        .iter()
        .map(|name| home.join(name))
        .collect();
    if let Some(project) = project {
        roots.extend(
            [".claude", ".codex", ".agents"]
                .iter()
                .map(|name| project.join(name)),
        );
    }
    roots
        .into_iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect()
}

/// The text of a command or skill file, without its front matter. Only inside the folders above, Markdown, 200 KB.
pub fn read(path: &Path, home: &Path, project: Option<&Path>) -> Result<String, String> {
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    if !allowed_roots(home, project)
        .iter()
        .any(|root| path.starts_with(root))
    {
        return Err("that file is not in an agent's command folder".to_string());
    }
    if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
        return Err("only Markdown files are commands".to_string());
    }
    if std::fs::metadata(&path)
        .map_err(|error| error.to_string())?
        .len()
        > 200 * 1024
    {
        return Err("the file is larger than 200 KB".to_string());
    }
    let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    Ok(split_front_matter(&text).1.to_owned())
}

fn project_of(cwd: &Option<String>) -> Option<PathBuf> {
    cwd.as_deref()
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .filter(|dir| dir.is_dir())
}

#[tauri::command]
pub async fn commands_list(
    agent: String,
    cwd: Option<String>,
) -> Result<Vec<ProviderCommand>, String> {
    let home = crate::home_folder()?;
    let project = project_of(&cwd);
    tauri::async_runtime::spawn_blocking(move || list(&agent, &home, project.as_deref()))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn command_read(path: String, cwd: Option<String>) -> Result<String, String> {
    let home = crate::home_folder()?;
    let project = project_of(&cwd);
    tauri::async_runtime::spawn_blocking(move || read(Path::new(&path), &home, project.as_deref()))
        .await
        .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("agentdeck-slash-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("makes the folder");
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("makes folders");
        std::fs::write(path, text).expect("writes");
    }

    #[test]
    fn a_block_description_is_read_whole_not_as_its_marker() {
        let (fields, _) = split_front_matter("---\nname: x\ndescription: >\n  First part\n  second part\nother: y\n---\nBody");
        assert_eq!(fields.get("description").map(String::as_str), Some("First part second part"));
        assert_eq!(fields.get("other").map(String::as_str), Some("y"));
        let (fields, _) = split_front_matter("---\ndescription: >\n---\nBody");
        assert_eq!(fields.get("description"), None);
    }

    #[test]
    fn front_matter_gives_fields_and_body() {
        let (fields, body) = split_front_matter(
            "---\nname: review\ndescription: \"Review a diff\"\n---\n\n# Body\nText",
        );
        assert_eq!(fields.get("name").map(String::as_str), Some("review"));
        assert_eq!(
            fields.get("description").map(String::as_str),
            Some("Review a diff")
        );
        assert_eq!(body, "# Body\nText");
        let (none, text) = split_front_matter("# Just text");
        assert!(none.is_empty());
        assert_eq!(text, "# Just text");
    }

    #[test]
    fn claude_commands_nest_and_skills_use_their_name() {
        let home = scratch("home");
        write(&home.join(".claude/commands/fix.md"), "Fix it: $ARGUMENTS");
        write(
            &home.join(".claude/commands/git/ship.md"),
            "---\ndescription: Ship it\n---\nRun the tests",
        );
        write(
            &home.join(".claude/skills/pdf/SKILL.md"),
            "---\nname: pdf\ndescription: Work with PDFs\n---\nSteps",
        );
        let project = scratch("project");
        write(&project.join(".claude/commands/fix.md"), "Project fix");
        let found = list("claude", &home, Some(&project));
        let names: Vec<_> = found
            .iter()
            .map(|item| (item.name.as_str(), item.kind, item.origin.as_str()))
            .collect();
        assert!(
            names.contains(&("fix", "prompt", "project")),
            "the project one wins: {names:?}"
        );
        assert_eq!(names.iter().filter(|item| item.0 == "fix").count(), 1);
        assert!(names.contains(&("git:ship", "prompt", "user")));
        assert!(names.contains(&("pdf", "skill", "user")));
        assert_eq!(
            found
                .iter()
                .find(|item| item.name == "git:ship")
                .map(|item| item.description.as_str()),
            Some("Ship it")
        );
    }

    #[test]
    fn only_command_folders_can_be_read() {
        let home = scratch("read-home");
        write(
            &home.join(".codex/prompts/a.md"),
            "---\ndescription: x\n---\nDo a: $ARGUMENTS",
        );
        write(&home.join("secret.md"), "nope");
        assert_eq!(
            read(&home.join(".codex/prompts/a.md"), &home, None).as_deref(),
            Ok("Do a: $ARGUMENTS")
        );
        assert!(read(&home.join("secret.md"), &home, None).is_err());
        write(&home.join(".codex/prompts/a.txt"), "x");
        assert!(read(&home.join(".codex/prompts/a.txt"), &home, None).is_err());
    }
}
