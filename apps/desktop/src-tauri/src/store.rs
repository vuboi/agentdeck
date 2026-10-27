//! The Store tab: the Tauri commands. Each one checks its input and hands the work to a module below.
//!
//! - `library`: the skill library and the junctions that give a skill to each agent.
//! - `plugins`: the plugins of Claude Code and Codex, through their own CLIs.
//! - `stage`, `scan`, `review`, `install`: "Add from outside" — fetch without running, scan, agent review, install.
//! - `catalog`: what can be installed (plugin marketplaces, the MCP registry, skill repositories).
//! - `lock`, `updates`: where each library skill came from, and updating it from there.
//! - `share`: using an installed plugin's skills and MCP servers with the other agents.
//!
//! Every install goes through `scan::gate`: a block stops it, a caution needs the user's acknowledgement.

use std::path::PathBuf;

use serde::Serialize;

pub mod catalog;
pub mod install;
pub mod library;
pub mod lock;
pub mod plugins;
pub mod review;
pub mod scan;
pub mod share;
pub mod stage;
pub mod updates;

use install::{checked_job, discard};
use library::{checked_name, SKILL_AGENTS};
use plugins::{check_plugin_id, run};

#[derive(Serialize, Debug, Default)]
pub struct StoreInstalled {
    pub skills: Vec<library::SkillItem>,
    pub plugins: Vec<plugins::PluginItem>,
    /// Why a CLI's plugin list could not be read; the rest still shows.
    pub plugin_errors: Vec<String>,
}

fn home() -> Result<PathBuf, String> {
    crate::home_folder()
}

/// Runs blocking file work off the async runtime.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())?
}

// ---------------------------------------------------------------------------------------------------------------
// Installed

#[tauri::command]
pub async fn store_installed() -> Result<StoreInstalled, String> {
    let home = home()?;
    let skills = blocking(move || Ok(library::installed_skills(&home))).await?;
    let mut installed = StoreInstalled {
        skills,
        ..StoreInstalled::default()
    };
    for agent in SKILL_AGENTS {
        match plugins::installed(agent).await {
            Ok(list) => installed.plugins.extend(list),
            Err(error) => installed.plugin_errors.push(format!("{agent}: {error}")),
        }
    }
    Ok(installed)
}

#[tauri::command]
pub async fn store_skill_import(name: String, from: String) -> Result<(), String> {
    blocking(move || library::import(&home()?, &name, &from)).await
}

#[tauri::command]
pub async fn store_skill_assign(name: String, agents: Vec<String>) -> Result<(), String> {
    blocking(move || library::assign(&home()?, &name, &agents)).await
}

#[tauri::command]
pub async fn store_skill_remove(name: String) -> Result<(), String> {
    blocking(move || library::remove(&home()?, &name)).await
}

#[tauri::command]
pub async fn store_plugin_set(agent: String, id: String, enabled: bool) -> Result<(), String> {
    check_plugin_id(&id)?;
    if agent != "claude" {
        return Err("Codex has no command to turn a plugin on or off; it can only be removed".to_owned());
    }
    let verb = if enabled { "enable" } else { "disable" };
    run(&agent, &["plugin", verb, id.as_str()]).await.map(|_| ())
}

#[tauri::command]
pub async fn store_plugin_uninstall(agent: String, id: String) -> Result<(), String> {
    check_plugin_id(&id)?;
    let verb = if agent == "codex" { "remove" } else { "uninstall" };
    run(&agent, &["plugin", verb, id.as_str()]).await.map(|_| ())
}

/// Uses an installed plugin's skills and MCP servers with the chosen agents.
#[tauri::command]
pub async fn store_plugin_share(agent: String, id: String, agents: Vec<String>, acknowledged: bool) -> Result<Vec<String>, String> {
    check_plugin_id(&id)?;
    share::share(&home()?, &agent, &id, &agents, acknowledged).await
}

/// Logs an MCP server in over OAuth. Codex opens the browser itself and waits for the redirect.
#[tauri::command]
pub async fn store_mcp_login(agent: String, name: String) -> Result<String, String> {
    if agent != "codex" {
        return Err("Claude Code asks for the OAuth login itself the first time the server is used".to_owned());
    }
    checked_name(&name)?;
    run("codex", &["mcp", "login", name.as_str()]).await.map(|out| out.trim().to_owned())
}

// ---------------------------------------------------------------------------------------------------------------
// Add from outside: stage, review, install

/// Scans a folder the user picked or one fetched into staging. Read-only: nothing in it runs.
#[tauri::command]
pub async fn store_scan_path(path: String) -> Result<scan::ScanReport, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("not a folder: {path}"));
    }
    blocking(move || Ok(scan::scan_folder(&root))).await
}

/// The native folder picker. The user picks the folder; nothing is read until it is staged.
#[tauri::command]
pub async fn store_pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    blocking(move || {
        Ok(app
            .dialog()
            .file()
            .set_title("Pick a skill or MCP folder")
            .blocking_pick_folder()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.to_string_lossy().into_owned()))
    })
    .await
}

#[tauri::command]
pub async fn store_stage(source: stage::Source) -> Result<install::StagedJob, String> {
    let home = home()?;
    let job_id = stage::new_job_id();
    let git_url = match &source {
        stage::Source::Git { url } => Some(url.clone()),
        _ => None,
    };
    let root = stage::stage(&home, &job_id, source).await?;
    // The address is kept with the job, so the install can record where the skill came from.
    if let Some(url) = git_url {
        install::remember_source(&home, &job_id, &url);
    }
    blocking(move || {
        let skills = stage::find_skills(&root);
        Ok(install::StagedJob {
            kind: install::kind_of(&root, &skills).to_owned(),
            skills: skills.into_iter().map(|(name, _)| name).collect(),
            scan: scan::scan_folder(&root),
            job_id,
        })
    })
    .await
}

/// Runs the agent review on a staged job and saves the answer. The verdict is the stricter of scan and review.
#[tauri::command]
pub async fn store_review(job_id: String, agent: String, model: String) -> Result<install::ReviewOutcome, String> {
    let job = checked_job(&job_id)?.to_owned();
    let home = home()?;
    let root = stage::staging_root(&home).join(&job);
    if !root.is_dir() {
        return Err("this job is no longer in staging".to_owned());
    }
    let scanned = root.clone();
    let scan_report = blocking(move || Ok(scan::scan_folder(&scanned))).await?;
    let prompt = review::build_prompt("skill, MCP config or plugin", &scan_report);
    let (answer, error) = match review::run(&agent, &model, &root, &prompt, &install::reply_file(&home, &job)).await {
        Ok(answer) => (Some(answer), None),
        Err(error) => (None, Some(error)),
    };
    let saved = install::SavedReview {
        review: answer.clone(),
        error: error.clone(),
    };
    std::fs::write(
        install::review_file(&home, &job),
        serde_json::to_string(&saved).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(install::ReviewOutcome {
        verdict: review::merge(scan_report.verdict, answer.as_ref()),
        scan: scan_report.verdict,
        review: answer,
        review_error: error,
    })
}

/// Installs a reviewed job, with the verdict the user saw (scan and saved review, the stricter one).
#[tauri::command]
pub async fn store_install_staged(job_id: String, agents: Vec<String>, acknowledged: bool) -> Result<Vec<String>, String> {
    let job = checked_job(&job_id)?.to_owned();
    let home = home()?;
    let root = stage::staging_root(&home).join(&job);
    let saved: install::SavedReview = std::fs::read_to_string(install::review_file(&home, &job))
        .map_err(|_| "this job has not been reviewed".to_owned())
        .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))?;
    let scanned = root.clone();
    let scan_report = blocking(move || Ok(scan::scan_folder(&scanned))).await?;
    scan::gate(review::merge(scan_report.verdict, saved.review.as_ref()), acknowledged, "this source")?;
    let lines = install::install_job(&home, &job, &root, &agents).await?;
    discard(&home, &job);
    Ok(lines)
}

/// Deletes a staged job and the files kept beside it. Called on cancel and after install.
#[tauri::command]
pub async fn store_discard_staged(job_id: String) -> Result<(), String> {
    let job = checked_job(&job_id)?.to_owned();
    discard(&home()?, &job);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Catalog and its sources

/// One page of the catalog that matches `filter`. The list is filtered and cut here, so the UI only ever holds the pages
/// it has scrolled to, not the thousands of items in all.
#[tauri::command]
pub async fn store_catalog_page(filter: catalog::Filter, offset: usize, limit: usize) -> Result<catalog::Page, String> {
    let home = home()?;
    blocking(move || Ok(catalog::page(&catalog::cached(&home), &filter, offset, limit.clamp(1, 200)))).await
}

/// Rebuilds the catalog from every source. Returns its totals (no items): the UI then asks for the first page again.
#[tauri::command]
pub async fn store_catalog_refresh() -> Result<catalog::Page, String> {
    let cache = catalog::refresh(&home()?).await?;
    Ok(catalog::page(&cache, &catalog::Filter::default(), 0, 0))
}

/// Installs one catalog item. The item's own install plan is the only thing run; the list is not trusted.
/// A plugin is scanned at its pinned source first, a skill after its clone. An MCP server runs a command, so it needs
/// `acknowledged` after the user has read the command.
#[tauri::command]
pub async fn store_catalog_install(item: catalog::CatalogItem, acknowledged: bool) -> Result<Vec<String>, String> {
    let home = home()?;
    match item.install {
        catalog::Install::Plugin { agent, plugin_id, source } => {
            check_plugin_id(&plugin_id)?;
            let job = stage::new_job_id();
            let scanned = catalog::plugin_folder(&home, &job, &source).await;
            let verdict = match scanned {
                Ok(folder) => blocking(move || Ok(scan::scan_folder(&folder).verdict)).await,
                Err(error) => Err(error),
            };
            discard(&home, &job);
            scan::gate(verdict?, acknowledged, &plugin_id)?;
            let verb = if agent == "codex" { "add" } else { "install" };
            run(&agent, &["plugin", verb, plugin_id.as_str()]).await.map(|out| vec![out.trim().to_owned()])
        }
        catalog::Install::Mcp { spec } => {
            scan::gate(scan::Verdict::Caution, acknowledged, &item.name)?;
            install::add_mcp(spec).await
        }
        catalog::Install::Skill { url, subpath } => {
            let job = stage::new_job_id();
            let result = install_catalog_skill(&home, &job, &url, &subpath, acknowledged).await;
            discard(&home, &job);
            result
        }
    }
}

async fn install_catalog_skill(home: &std::path::Path, job: &str, url: &str, subpath: &str, acknowledged: bool) -> Result<Vec<String>, String> {
    let root = stage::stage(home, job, stage::Source::Git { url: url.to_owned() }).await?;
    let folder = root.join(subpath);
    if !folder.join("SKILL.md").is_file() {
        return Err("the source no longer has a SKILL.md there".to_owned());
    }
    let name = folder.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_owned();
    let scanned = folder.clone();
    let verdict = blocking(move || Ok(scan::scan_folder(&scanned).verdict)).await?;
    scan::gate(verdict, acknowledged, &name)?;
    let agents: Vec<String> = SKILL_AGENTS.iter().map(|agent| (*agent).to_owned()).collect();
    library::add(home, &name, &folder, url, subpath, &agents)?;
    Ok(vec![format!("{name} installed")])
}

#[tauri::command]
pub async fn store_sources_list() -> Result<Vec<String>, String> {
    Ok(catalog::read_sources(&home()?))
}

/// Adds a skill source (an https git address). Nothing is fetched now: the next refresh reads it.
#[tauri::command]
pub async fn store_source_add(url: String) -> Result<Vec<String>, String> {
    let home = home()?;
    let url = catalog::checked_source(&url)?;
    let mut sources = catalog::read_sources(&home);
    if !sources.contains(&url) {
        sources.push(url);
        catalog::write_sources(&home, &sources)?;
    }
    Ok(sources)
}

#[tauri::command]
pub async fn store_source_remove(url: String) -> Result<Vec<String>, String> {
    let home = home()?;
    let mut sources = catalog::read_sources(&home);
    sources.retain(|item| item != &url);
    catalog::write_sources(&home, &sources)?;
    Ok(sources)
}

// ---------------------------------------------------------------------------------------------------------------
// Updates

/// Skills from git sources, each compared with its source now.
#[tauri::command]
pub async fn store_updates_check() -> Result<Vec<updates::UpdateItem>, String> {
    Ok(updates::check(&home()?).await)
}

#[tauri::command]
pub async fn store_update_skill(name: String, acknowledged: bool) -> Result<(), String> {
    let name = checked_name(&name)?.to_owned();
    updates::apply(&home()?, &name, acknowledged).await
}
