//! The skill library. `~/.agentdeck/library/skills/<name>` holds one copy of each skill; every agent given the skill gets
//! a directory junction in its own skills folder that points at that copy. A folder the user made by hand under the
//! same name is never overwritten or removed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::lock;
use crate::slash_commands;

/// The agents that read skills from a folder AgentDeck can write to.
pub const SKILL_AGENTS: [&str; 5] = ["claude", "codex", "opencode", "antigravity", "copilot"];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SkillItem {
    pub name: String,
    pub description: String,
    /// `library` (AgentDeck's copy), `user` (a folder of one agent) or the name of a plugin.
    pub origin: String,
    pub path: String,
    /// The agents that can read it.
    pub agents: Vec<String>,
    /// The agents that get the library copy through a junction.
    pub linked: Vec<String>,
    /// Only a library or user skill can be given to agents; a plugin skill belongs to its plugin.
    pub assignable: bool,
}

pub fn library_root(home: &Path) -> PathBuf {
    home.join(".agentdeck").join("library").join("skills")
}

/// The folders where an agent keeps its own skills. The first one is where AgentDeck links new skills.
fn skill_folders(home: &Path, agent: &str) -> Vec<PathBuf> {
    match agent {
        "claude" => vec![home.join(".claude").join("skills")],
        "codex" => vec![
            home.join(".codex").join("skills"),
            home.join(".agents").join("skills"),
        ],
        // OpenCode's own folders (`{skill,skills}/**/SKILL.md` under its config). It also reads `~/.claude/skills` and
        // `~/.agents/skills`, so a skill given to Claude or Codex is already visible to it.
        "opencode" => vec![
            home.join(".config").join("opencode").join("skills"),
            home.join(".config").join("opencode").join("skill"),
        ],
        // Antigravity (agy) reads the skills of its config folder.
        "antigravity" => vec![home.join(".gemini").join("config").join("skills")],
        "copilot" => vec![home.join(".copilot").join("skills")],
        _ => Vec::new(),
    }
}

/// Project destinations per provider. Keep this in sync with native discovery and the provider checklist.
pub fn project_skill_folders(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &[".claude/skills"],
        "codex" => &[".codex/skills", ".agents/skills"],
        "copilot" => &[".github/skills"],
        "opencode" => &[".opencode/skills"],
        "antigravity" => &[".agents/skills"],
        _ => &[],
    }
}

/// Every supported project destination, deduplicating folders shared by providers.
pub fn project_folders() -> std::collections::BTreeSet<&'static str> {
    SKILL_AGENTS.iter().flat_map(|agent| project_skill_folders(agent).iter().copied()).collect()
}

/// A skill name is one folder name: no separators, no `.` or `..`.
pub fn checked_name(name: &str) -> Result<&str, String> {
    let bad = name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']);
    if bad {
        Err(format!("not a valid skill name: {name}"))
    } else {
        Ok(name)
    }
}

/// A path as plain lowercase text, without the `\\?\` prefix Windows adds to some reads.
fn key(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('/', "\\")
        .to_lowercase()
}

/// True when `link` is a junction or symlink whose target lies inside `root`.
fn points_into(link: &Path, root: &Path) -> bool {
    let Ok(target) = std::fs::read_link(link) else {
        return false;
    };
    key(&target).starts_with(&format!("{}\\", key(root)))
}

fn push_unique(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|item| item == value) {
        list.push(value.to_owned());
    }
}

/// Makes `link` a directory junction to `target`. Windows junctions need no admin rights.
pub(super) fn link_dir(link: &Path, target: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let message = if stderr.is_empty() {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        } else {
            stderr
        };
        Err(message)
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(target, link).map_err(|error| error.to_string())
    }
}

/// Removes a junction or symlink and leaves its target alone.
fn unlink(link: &Path) -> std::io::Result<()> {
    std::fs::remove_dir(link).or_else(|_| std::fs::remove_file(link))
}

/// Copies a skill folder. Links inside it are skipped, so nothing outside the folder is copied.
pub fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source = entry.path();
        if std::fs::read_link(&source).is_ok() {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|error| error.to_string())?.is_dir() {
            copy_tree(&source, &target)?;
        } else {
            std::fs::copy(&source, &target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Every skill the agents can read: the library, the agents' own folders, and the skills of plugins.
pub fn installed_skills(home: &Path) -> Vec<SkillItem> {
    let library = library_root(home);
    let mut items: BTreeMap<String, SkillItem> = BTreeMap::new();

    let mut found = Vec::new();
    slash_commands::skills_in(&library, "library", "", &mut found);
    for command in found {
        items.insert(
            command.name.clone(),
            SkillItem {
                name: command.name,
                description: command.description,
                origin: "library".to_owned(),
                path: command.path,
                agents: Vec::new(),
                linked: Vec::new(),
                assignable: true,
            },
        );
    }

    for agent in SKILL_AGENTS {
        for folder in skill_folders(home, agent) {
            let mut found = Vec::new();
            slash_commands::skills_in(&folder, "user", "", &mut found);
            for command in found {
                let linked = points_into(&folder.join(&command.name), &library);
                let item = items
                    .entry(command.name.clone())
                    .or_insert_with(|| SkillItem {
                        name: command.name.clone(),
                        description: command.description.clone(),
                        origin: "user".to_owned(),
                        path: command.path.clone(),
                        agents: Vec::new(),
                        linked: Vec::new(),
                        assignable: true,
                    });
                push_unique(&mut item.agents, agent);
                if linked {
                    push_unique(&mut item.linked, agent);
                }
            }
        }
    }

    let mut plugin_skills = Vec::new();
    for agent in SKILL_AGENTS {
        for command in slash_commands::list(agent, home, None) {
            if command.kind != "skill" || command.origin == "user" || command.origin == "project" {
                continue;
            }
            plugin_skills.push(SkillItem {
                name: command.name,
                description: command.description,
                origin: command.origin,
                path: command.path,
                agents: vec![agent.to_owned()],
                linked: Vec::new(),
                assignable: false,
            });
        }
    }

    items.into_values().chain(plugin_skills).collect()
}

/// Copies a skill from one agent's folder into the library. The original stays where it is.
pub fn import(home: &Path, name: &str, from: &str) -> Result<(), String> {
    let name = checked_name(name)?;
    let target = library_root(home).join(name);
    if target.exists() {
        return Err(format!("{name} is already in the library"));
    }
    let source = skill_folders(home, from)
        .into_iter()
        .map(|folder| folder.join(name))
        .find(|path| path.join("SKILL.md").is_file())
        .ok_or_else(|| format!("{from} has no skill named {name}"))?;
    if let Err(error) = copy_tree(&source, &target) {
        let _ = std::fs::remove_dir_all(&target);
        return Err(error);
    }
    lock::record(home, name, "folder", "", &target)
}

/// Puts one skill folder into the library, records where it came from, and gives it to `agents`. Every install path
/// (a staged job, the catalog, a shared plugin) ends here.
pub fn add(home: &Path, name: &str, folder: &Path, source: &str, subpath: &str, agents: &[String]) -> Result<(), String> {
    let name = checked_name(name)?;
    let target = library_root(home).join(name);
    if target.exists() {
        return Err(format!("{name} is already in the library"));
    }
    if let Err(error) = copy_tree(folder, &target) {
        let _ = std::fs::remove_dir_all(&target);
        return Err(error);
    }
    lock::record(home, name, source, subpath, &target)?;
    assign(home, name, agents)
}

/// Gives the library skill to the listed agents and takes it from the others. Real folders are never touched.
pub fn assign(home: &Path, name: &str, agents: &[String]) -> Result<(), String> {
    let name = checked_name(name)?;
    let library = library_root(home);
    let source = library.join(name);
    if !source.join("SKILL.md").is_file() {
        return Err(format!("{name} is not in the library"));
    }
    for agent in SKILL_AGENTS {
        let Some(folder) = skill_folders(home, agent).into_iter().next() else {
            continue;
        };
        let link = folder.join(name);
        let wanted = agents.iter().any(|item| item == agent);
        let linked = points_into(&link, &library);
        if wanted && !linked {
            if link.symlink_metadata().is_ok() {
                return Err(format!("{agent} already has its own {name} folder; it is not overwritten"));
            }
            std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
            link_dir(&link, &source)?;
        } else if !wanted && linked {
            unlink(&link).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Takes the library skill away from every agent, then deletes the library copy and its lock entry.
pub fn remove(home: &Path, name: &str) -> Result<(), String> {
    let name = checked_name(name)?;
    let library = library_root(home);
    for agent in SKILL_AGENTS {
        for folder in skill_folders(home, agent) {
            let link = folder.join(name);
            if points_into(&link, &library) {
                unlink(&link).map_err(|error| error.to_string())?;
            }
        }
    }
    for project in super::sync::known_projects(home) {
        for folder in project_folders() {
            let link = project.join(folder).join(name);
            if points_into(&link, &library) { unlink(&link).map_err(|e| e.to_string())?; }
        }
    }
    let source = library.join(name);
    if source.exists() {
        std::fs::remove_dir_all(&source).map_err(|error| error.to_string())?;
    }
    lock::forget(home, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("agentdeck-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("makes the folder");
        dir
    }

    fn write_skill(folder: &Path, name: &str, description: &str) {
        let dir = folder.join(name);
        std::fs::create_dir_all(&dir).expect("makes the skill folder");
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\nBody\n"),
        )
        .expect("writes SKILL.md");
    }

    #[test]
    fn each_provider_has_explicit_global_and_project_skill_destinations() {
        for agent in SKILL_AGENTS {
            assert!(!skill_folders(Path::new("user"), agent).is_empty(), "global: {agent}");
            assert!(!project_skill_folders(agent).is_empty(), "project: {agent}");
        }
        let folders = project_folders();
        assert!(folders.contains(".codex/skills"));
        assert!(folders.contains(".claude/skills"));
        assert!(folders.contains(".github/skills"));
        assert!(folders.contains(".opencode/skills"));
        assert_eq!(folders.len(), 5);
        assert!(project_skill_folders("unknown").is_empty());
    }

    #[test]
    fn names_with_separators_are_refused() {
        assert!(checked_name("good-skill").is_ok());
        assert!(checked_name("../escape").is_err());
        assert!(checked_name("a\\b").is_err());
        assert!(checked_name("").is_err());
    }

    #[test]
    fn import_copies_and_remove_deletes_only_the_library_copy() {
        let home = scratch("import");
        write_skill(&home.join(".claude").join("skills"), "notes", "Take notes");

        import(&home, "notes", "claude").expect("imports");
        assert!(library_root(&home).join("notes").join("SKILL.md").is_file());
        assert!(home.join(".claude").join("skills").join("notes").join("SKILL.md").is_file());
        assert!(lock::read(&home).contains_key("notes"), "the import is recorded");

        let skills = installed_skills(&home);
        let notes = skills.iter().find(|item| item.name == "notes").expect("listed");
        assert_eq!(notes.origin, "library");
        assert_eq!(notes.agents, vec!["claude".to_owned()]);
        assert!(notes.linked.is_empty());

        assert!(import(&home, "notes", "claude").is_err(), "a second import is refused");
        remove(&home, "notes").expect("removes");
        assert!(!library_root(&home).join("notes").exists());
        assert!(!lock::read(&home).contains_key("notes"), "its lock entry goes too");
        assert!(home.join(".claude").join("skills").join("notes").join("SKILL.md").is_file(), "the agent's own copy stays");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn assign_never_overwrites_a_real_folder() {
        let home = scratch("assign");
        write_skill(&library_root(&home), "shared", "Shared");
        write_skill(&home.join(".codex").join("skills"), "shared", "Mine");

        let error = assign(&home, "shared", &["codex".to_owned()]).expect_err("refuses to overwrite");
        assert!(error.contains("codex"));
        let _ = std::fs::remove_dir_all(&home);
    }
}
