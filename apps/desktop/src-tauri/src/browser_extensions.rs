//! The extensions of the Preview browser (Windows, WebView2): what the toolbar's extension menu lists, turns on or off,
//! reloads and removes. Adding is in `browser.rs` (`browser_add_extension`).
//!
//! Every unpacked extension lives in its own folder under `<app data>/browser-extensions`; the Preview loads that folder
//! when a tab's webview is created (`extensions_path`), and an extension added while a tab is open is installed in the
//! profile at once. The profile knows an extension by its id, the folder by its name, so the add writes the pair to
//! `ids.json` there. Which ones the user turned off is kept in `disabled.json`, and put back on the profile whenever the
//! list is read: loading the folder again at the next start would otherwise turn them on.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

/// One extension, as the menu shows it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct BrowserExtension {
    /// The profile's id; empty for an extension only on disk (no page open yet).
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// The page its toolbar button opens (`action.default_popup`), as a `chrome-extension://` address.
    pub popup: Option<String>,
    /// Its icon (the largest of the manifest's `icons`), as a `data:` URL; none for one AgentDeck has no folder of.
    pub icon: Option<String>,
}

/// What one extension folder's manifest says.
#[derive(Clone, Debug, Default)]
struct Manifest {
    name: String,
    popup: Option<String>,
    icon: Option<String>,
}

/// The largest icon a manifest names (`icons`, else the toolbar's `default_icon`), read as a `data:` URL.
fn icon_of(folder: &Path, manifest: &serde_json::Value) -> Option<String> {
    use base64::Engine;

    let largest = |icons: &serde_json::Value| {
        icons
            .as_object()?
            .iter()
            .filter_map(|(size, path)| Some((size.parse::<u32>().unwrap_or(0), path.as_str()?)))
            .max_by_key(|(size, _)| *size)
            .map(|(_, path)| path.to_owned())
    };
    let action = ["action", "browser_action", "page_action"].iter().map(|key| &manifest[key]["default_icon"]).find(|icon| !icon.is_null());
    let path = largest(&manifest["icons"])
        .or_else(|| action.and_then(|icon| icon.as_str().map(str::to_owned).or_else(|| largest(icon))))?;
    let file = folder.join(path.trim_start_matches('/'));
    // Inside its own folder only, and small: it is drawn at 16 px.
    if !file.starts_with(folder) || std::fs::metadata(&file).ok()?.len() > 512 * 1024 {
        return None;
    }
    let mime = match file.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => return None,
    };
    let bytes = std::fs::read(&file).ok()?;
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

pub fn root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|error| error.to_string())?.join("browser-extensions"))
}

fn read_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read_to_string(path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    std::fs::write(path, serde_json::to_string_pretty(value).map_err(|error| error.to_string())?).map_err(|error| error.to_string())
}

/// Id to folder name, written when an extension is added.
fn ids(root: &Path) -> BTreeMap<String, String> {
    read_json(&root.join("ids.json"))
}

pub fn remember_id(root: &Path, id: &str, folder: &str) -> Result<(), String> {
    let mut known = ids(root);
    known.insert(id.to_owned(), folder.to_owned());
    write_json(&root.join("ids.json"), &known)
}

fn disabled(root: &Path) -> BTreeSet<String> {
    read_json(&root.join("disabled.json"))
}

/// The manifest of one extension folder: its name, toolbar page and icon.
fn manifest_of(folder: &Path) -> Option<Manifest> {
    let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(folder.join("manifest.json")).ok()?).ok()?;
    let name = manifest["name"].as_str()?.to_owned();
    let popup = ["action", "browser_action", "page_action"]
        .iter()
        .find_map(|key| manifest[key]["default_popup"].as_str())
        .map(|page| page.trim_start_matches('/').to_owned());
    Some(Manifest { name, popup, icon: icon_of(folder, &manifest) })
}

/// The extension folders on disk (each with a manifest), by folder name.
fn folders(root: &Path) -> Vec<(String, Manifest)> {
    let mut found: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| Some((entry.file_name().to_string_lossy().into_owned(), manifest_of(&entry.path())?)))
        .collect();
    found.sort_by_key(|(_, manifest)| manifest.name.to_lowercase());
    found
}

/// The list the menu shows: the profile's extensions (`live`: id, name, enabled) joined with what the folders say, or the
/// folders alone when no page is open. An extension of the profile without a known folder is listed too.
fn listed(root: &Path, live: Option<Vec<(String, String, bool)>>) -> Vec<BrowserExtension> {
    let off = disabled(root);
    let known = ids(root);
    let on_disk = folders(root);
    // The folder of a profile extension: the one its id was recorded with, else one whose manifest has its name.
    let folder_of = |id: &str, name: &str| {
        let folder = known.get(id);
        on_disk
            .iter()
            .find(|(dir, manifest)| folder.is_some_and(|folder| folder == dir) || manifest.name == name)
            .map(|(_, manifest)| manifest.clone())
            .unwrap_or_default()
    };
    match live {
        Some(live) => live
            .into_iter()
            .map(|(id, name, enabled)| {
                let manifest = folder_of(&id, &name);
                BrowserExtension {
                    popup: manifest.popup.map(|page| format!("chrome-extension://{id}/{page}")),
                    icon: manifest.icon,
                    id,
                    name,
                    enabled,
                }
            })
            .collect(),
        None => on_disk
            .into_iter()
            .map(|(dir, manifest)| {
                let id = known.iter().find(|(_, folder)| **folder == dir).map(|(id, _)| id.clone()).unwrap_or_default();
                BrowserExtension { enabled: !off.contains(&id), popup: None, icon: manifest.icon, id, name: manifest.name }
            })
            .collect(),
    }
}

#[cfg(windows)]
mod profile {
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2BrowserExtension, ICoreWebView2Profile7, ICoreWebView2_13};
    use webview2_com::{BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler, ProfileGetBrowserExtensionsCompletedHandler};
    use windows::core::{Interface, BOOL, PWSTR};

    use std::cell::RefCell;
    use std::rc::Rc;

    /// The Preview profile of a tab's webview.
    pub unsafe fn of(platform: &tauri::webview::PlatformWebview) -> Result<ICoreWebView2Profile7, String> {
        unsafe {
            platform
                .controller()
                .CoreWebView2()
                .and_then(|webview| webview.cast::<ICoreWebView2_13>())
                .and_then(|webview| webview.Profile())
                .and_then(|profile| profile.cast::<ICoreWebView2Profile7>())
                .map_err(|error| error.to_string())
        }
    }

    pub unsafe fn extensions(profile: &ICoreWebView2Profile7) -> Result<Vec<ICoreWebView2BrowserExtension>, String> {
        let found = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&found);
        let profile = profile.clone();
        ProfileGetBrowserExtensionsCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| unsafe { profile.GetBrowserExtensions(&handler) }.map_err(webview2_com::Error::WindowsError)),
            Box::new(move |error, list| {
                error?;
                if let Some(list) = list {
                    let mut count = 0u32;
                    unsafe { list.Count(&mut count) }?;
                    for index in 0..count {
                        sink.borrow_mut().push(unsafe { list.GetValueAtIndex(index) }?);
                    }
                }
                Ok(())
            }),
        )
        .map_err(|error| error.to_string())?;
        Ok(found.take())
    }

    /// Id, name and whether it is on.
    pub unsafe fn describe(extension: &ICoreWebView2BrowserExtension) -> Result<(String, String, bool), String> {
        let (mut id, mut name, mut enabled) = (PWSTR::null(), PWSTR::null(), BOOL::default());
        unsafe {
            extension.Id(&mut id).map_err(|error| error.to_string())?;
            extension.Name(&mut name).map_err(|error| error.to_string())?;
            extension.IsEnabled(&mut enabled).map_err(|error| error.to_string())?;
        }
        Ok((webview2_com::take_pwstr(id), webview2_com::take_pwstr(name), enabled.as_bool()))
    }

    pub unsafe fn enable(extension: &ICoreWebView2BrowserExtension, on: bool) -> Result<(), String> {
        let extension = extension.clone();
        BrowserExtensionEnableCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| unsafe { extension.Enable(on, &handler) }.map_err(webview2_com::Error::WindowsError)),
            Box::new(|error| error),
        )
        .map_err(|error| error.to_string())
    }

    pub unsafe fn remove(extension: &ICoreWebView2BrowserExtension) -> Result<(), String> {
        let extension = extension.clone();
        BrowserExtensionRemoveCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| unsafe { extension.Remove(&handler) }.map_err(webview2_com::Error::WindowsError)),
            Box::new(|error| error),
        )
        .map_err(|error| error.to_string())
    }
}

/// The Preview profile of a tab's webview (see `browser_add_extension`).
#[cfg(windows)]
pub unsafe fn profile_of(
    platform: &tauri::webview::PlatformWebview,
) -> Result<webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Profile7, String> {
    unsafe { profile::of(platform) }
}

/// Runs `work` on the profile of the tab's webview (on its thread), or returns `None` when the tab has no webview.
#[cfg(windows)]
async fn on_profile<T: Send + 'static>(
    app: &AppHandle,
    tab: &str,
    work: impl FnOnce(&webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Profile7) -> Result<T, String> + Send + 'static,
) -> Result<Option<T>, String> {
    let Ok(view) = crate::browser::webview(app, tab) else {
        return Ok(None);
    };
    let (sender, receiver) = tokio::sync::oneshot::channel();
    view.with_webview(move |platform| {
        let result = unsafe { profile::of(&platform) }.and_then(|profile| work(&profile));
        let _ = sender.send(result);
    })
    .map_err(|error| error.to_string())?;
    receiver.await.map_err(|error| error.to_string())?.map(Some)
}

/// The extensions for the toolbar menu. Puts the user's "off" back on extensions the start-up load turned on.
#[tauri::command]
pub async fn browser_extensions(app: AppHandle, tab: String) -> Result<Vec<BrowserExtension>, String> {
    let root = root(&app)?;
    #[cfg(windows)]
    {
        let off = disabled(&root);
        let live = on_profile(&app, &tab, move |profile| unsafe {
            let mut out = Vec::new();
            for extension in profile::extensions(profile)? {
                let (id, name, mut enabled) = profile::describe(&extension)?;
                if enabled && off.contains(&id) {
                    profile::enable(&extension, false)?;
                    enabled = false;
                }
                out.push((id, name, enabled));
            }
            Ok(out)
        })
        .await?;
        Ok(listed(&root, live))
    }
    #[cfg(not(windows))]
    {
        let _ = tab;
        Ok(listed(&root, None))
    }
}

/// What to do to one extension of the profile.
#[derive(Clone, Copy)]
enum Change {
    Enable(bool),
    Reload,
    Remove,
}

async fn change(app: &AppHandle, tab: &str, id: String, what: Change) -> Result<(), String> {
    let root = root(app)?;
    #[cfg(windows)]
    {
        let target = id.clone();
        let done = on_profile(app, tab, move |profile| unsafe {
            let extension = profile::extensions(profile)?
                .into_iter()
                .find(|extension| profile::describe(extension).is_ok_and(|(found, _, _)| found == target))
                .ok_or("That extension is not in the browser any more.")?;
            match what {
                Change::Enable(on) => profile::enable(&extension, on),
                // Turned off and on again: the extension's pages and background script start over from its files.
                Change::Reload => profile::enable(&extension, false).and_then(|()| profile::enable(&extension, true)),
                Change::Remove => profile::remove(&extension),
            }
        })
        .await?;
        if done.is_none() {
            return Err("Open a page in the browser first: the extension is changed in its profile.".into());
        }
    }
    #[cfg(not(windows))]
    {
        let _ = tab;
        return Err("Browser extensions are currently supported only on Windows.".into());
    }
    let mut off = disabled(&root);
    match what {
        Change::Enable(on) => {
            if on {
                off.remove(&id);
            } else {
                off.insert(id.clone());
            }
        }
        Change::Reload => {}
        Change::Remove => {
            off.remove(&id);
            let mut known = ids(&root);
            // Its folder goes too, or the next start would load it again.
            if let Some(folder) = known.remove(&id) {
                let _ = std::fs::remove_dir_all(root.join(folder));
            }
            write_json(&root.join("ids.json"), &known)?;
        }
    }
    write_json(&root.join("disabled.json"), &off)
}

#[tauri::command]
pub async fn browser_extension_enable(app: AppHandle, tab: String, id: String, enabled: bool) -> Result<(), String> {
    change(&app, &tab, id, Change::Enable(enabled)).await
}

#[tauri::command]
pub async fn browser_extension_reload(app: AppHandle, tab: String, id: String) -> Result<(), String> {
    change(&app, &tab, id, Change::Reload).await
}

#[tauri::command]
pub async fn browser_extension_remove(app: AppHandle, tab: String, id: String) -> Result<(), String> {
    change(&app, &tab, id, Change::Remove).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(root: &Path, dir: &str, manifest: &str) {
        std::fs::create_dir_all(root.join(dir)).unwrap();
        std::fs::write(root.join(dir).join("manifest.json"), manifest).unwrap();
    }

    #[test]
    fn the_list_joins_the_profile_with_the_folders() {
        let root = std::env::temp_dir().join(format!("agentdeck-ext-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        folder(&root, "extension-a", r#"{"manifest_version":3,"name":"Alpha","action":{"default_popup":"/popup.html"},"icons":{"16":"i16.png","128":"i128.png"}}"#);
        std::fs::write(root.join("extension-a").join("i128.png"), [1u8, 2, 3]).unwrap();
        folder(&root, "extension-b", r#"{"manifest_version":3,"name":"Beta"}"#);
        remember_id(&root, "aaa", "extension-a").unwrap();

        let live = listed(&root, Some(vec![("aaa".into(), "Alpha".into(), true), ("bbb".into(), "Beta".into(), false)]));
        assert_eq!(live[0].popup.as_deref(), Some("chrome-extension://aaa/popup.html"));
        assert_eq!(live[0].icon.as_deref(), Some("data:image/png;base64,AQID"), "the largest icon");
        assert_eq!(live[1].popup, None);
        assert_eq!(live[1].icon, None);
        assert!(!live[1].enabled);

        write_json(&root.join("disabled.json"), &["aaa"]).unwrap();
        let on_disk = listed(&root, None);
        assert_eq!(on_disk.iter().map(|item| (item.name.as_str(), item.enabled)).collect::<Vec<_>>(), [("Alpha", false), ("Beta", true)]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
