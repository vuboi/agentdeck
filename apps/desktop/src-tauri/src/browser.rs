//! The AgentDeck browser: one child webview per tab, laid over the preview panel.
//!
//! Each page runs in its own webview, so a tab behaves like a normal browser tab. A script injected into every
//! page (browser/inject.js) reports console, network and navigation, and answers requests for the DOM, a
//! screenshot and the element picker. Reports come back through the `agentdeck` URL scheme and are forwarded
//! to the app window as `browser-report` events; page loads are forwarded as `browser-load` events.

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(windows)]
use std::path::{Path, PathBuf};

use tauri::webview::PageLoadEvent;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl,
};

const PREFIX: &str = "browser-";
/// The main app and Preview must agree because WebView2 shares one user data folder.
pub const EXTENSIONS_ENABLED: bool = cfg!(windows);

/// Opening can be asked for twice at once (resize and navigation); only one may create a webview.
static CREATING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The address each tab asked for, by tab id. A failed load shows the tab's own error page at this address.
static REQUESTED: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> =
    std::sync::Mutex::new(None);

/// Marks the error page AgentDeck shows in place of the browser's own (Chromium's `chrome-error://` page).
const ERROR_MARK: &str = "<!--agentdeck-error-->";

/// How many times a tab's failed page was asked for again, and whether one such retry is running now, by tab id.
/// More than `RETRIES` means it gave up and shows AgentDeck's error page. A page that loads, or an address the user
/// asks for, starts the count again.
static FAILURES: std::sync::Mutex<Option<std::collections::HashMap<String, (u8, bool)>>> =
    std::sync::Mutex::new(None);

/// How often a page the engine could not load is asked for again (after 1 s, 2 s, 4 s) before the error page.
const RETRIES: u8 = 3;

fn failures<R>(
    f: impl FnOnce(&mut std::collections::HashMap<String, (u8, bool)>) -> R,
) -> Option<R> {
    FAILURES
        .lock()
        .ok()
        .map(|mut map| f(map.get_or_insert_with(Default::default)))
}

fn requested(tab: &str) -> Option<String> {
    REQUESTED.lock().ok()?.as_ref()?.get(tab).cloned()
}

fn remember(tab: &str, url: &str) {
    if let Ok(mut map) = REQUESTED.lock() {
        map.get_or_insert_with(Default::default)
            .insert(tab.to_string(), url.to_string());
    }
}

/// AgentDeck's page for an address that did not answer, like a browser's own: the tab keeps the address, Reload
/// tries again, and the page opens the address by itself as soon as its server answers. It is served by the
/// `agentdeck` scheme (WebView2 does not show `data:` pages here).
pub(crate) fn error_html(target: &str, reason: &str) -> String {
    let (what, code) = what_happened(target, reason);
    let shown = target
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;");
    let host = tauri::Url::parse(target)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| shown.clone());
    let go = target
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('"', "%22");
    format!(
        "{ERROR_MARK}<!doctype html><html><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>{host}</title><style>:root{{color-scheme:light dark;--bg:#ffffff;--fg:#202124;--muted:#5f6368;--accent:#1a73e8}}@media (prefers-color-scheme:dark){{:root{{--bg:#202124;--fg:#e8eaed;--muted:#9aa0a6;--accent:#8ab4f8}}}}body{{margin:0;min-height:100vh;background:var(--bg);color:var(--muted);font:15px/1.55 system-ui,'Segoe UI',sans-serif}}main{{max-width:560px;margin:0 auto;padding:18vh 32px 32px}}svg{{width:40px;height:40px;color:var(--accent)}}h1{{margin:24px 0 12px;font-size:22px;font-weight:500;color:var(--fg)}}ul{{margin:8px 0 0;padding-left:22px}}code{{font-size:12px;letter-spacing:.02em}}button{{margin-top:32px;padding:8px 18px;border:0;border-radius:6px;background:var(--accent);color:var(--bg);font:500 14px system-ui,sans-serif;cursor:pointer}}</style><main><svg viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round'><path d='M12 21a9 9 0 1 1 9-9'/><path d='M12 7v5l3 2'/></svg><h1>This site can't be reached</h1><p>{what}</p><p>Try:</p><ul><li>Checking that the dev server is running</li><li>Checking the address, and whether it is http or https: <code>{shown}</code></li></ul><p><code>{code}</code></p><button onclick=\"location.replace('{go}')\">Reload</button></main><script>setInterval(()=>fetch('{go}',{{mode:'no-cors',cache:'no-store'}}).then(()=>location.replace('{go}'),()=>{{}}),1500)</script></html>"
    )
}

/// What the error page tells about `target` (HTML) and the code under it. `reason` is `closed` (nothing answers) or
/// anything else (it answered, but the page could not be loaded).
fn what_happened(target: &str, reason: &str) -> (String, &'static str) {
    let url = tauri::Url::parse(target).ok();
    let host = url
        .as_ref()
        .and_then(|url| url.host_str())
        .unwrap_or("this address")
        .replace('<', "&lt;");
    let local_port = url
        .as_ref()
        .filter(|url| is_local(url))
        .and_then(|url| url.port_or_known_default());
    match (reason, local_port) {
        ("closed", Some(port)) => (format!("Nothing answers on port {port} of this machine (<b>{host}</b>). Is the dev server running?"), "ERR_CONNECTION_REFUSED"),
        ("closed", None) => (format!("<b>{host}</b> refused to connect or did not answer."), "ERR_CONNECTION_REFUSED"),
        _ => (format!("<b>{host}</b> did not answer properly: the connection was reset or closed, or its certificate was refused."), "ERR_CONNECTION_FAILED"),
    }
}

/// The address of AgentDeck's error page for `target` (`reason`: see `what_happened`).
fn error_page(target: &str, reason: &str) -> Result<tauri::Url, String> {
    tauri::Url::parse_with_params(
        &format!("{}error", base()),
        &[("url", target), ("reason", reason)],
    )
    .map_err(|error| error.to_string())
}

/// Where the page posts its reports: the custom scheme, which WebView2 serves under a localhost name.
fn base() -> &'static str {
    if cfg!(windows) {
        "http://agentdeck.localhost/"
    } else {
        "agentdeck://localhost/"
    }
}

fn injected_script(tab: &str) -> String {
    format!(
        "{}\n{}",
        include_str!("../browser/html-to-image.js"),
        include_str!("../browser/inject.js")
            .replace("__BASE__", base())
            .replace("__TAB__", tab)
    )
}

/// The handler of the `agentdeck` scheme: forwards a page's JSON report to the app window.
pub fn handle_report(app: &AppHandle, body: &[u8]) {
    if let Ok(report) = serde_json::from_slice::<serde_json::Value>(body) {
        if report["kind"] == "nav" {
            let tab = report["tab"].as_str().unwrap_or_default();
            let url = report["url"].as_str().unwrap_or_default();
            // The injected script also runs on the engine's own error page (chrome-error://chromewebdata/): the page
            // could not be loaded. It is asked for again, then replaced by AgentDeck's error page; the tab keeps
            // showing the address that was asked for.
            if url.starts_with("chrome-error:") {
                navigation_failed(app, tab);
                return;
            }
            // AgentDeck's own error page is not an address: the tab keeps the one that was asked for.
            if url.starts_with(&format!("{}error", base())) {
                return;
            }
            failures(|map| map.remove(tab));
        }
        crate::bridge::ingest(&report);
        #[cfg(not(windows))]
        {
            if report["kind"] == "network" {
                fallback::network_events(app, &report);
            }
        }
        let _ = app.emit_to("main", "browser-report", report);
    }
}

/// Tab ids come from the app; keep them to a safe label.
fn label(tab: &str) -> String {
    let safe: String = tab
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    format!("{PREFIX}{safe}")
}

pub(crate) fn webview(app: &AppHandle, tab: &str) -> Result<Webview, String> {
    app.get_webview(&label(tab))
        .ok_or_else(|| "the browser is not open".to_string())
}

/// Whether a local address (localhost, 127.0.0.1, [::1]) has a server on its port. Other addresses count as up:
/// only the page itself can tell. Vite may listen on IPv4 or IPv6 only, so both are tried.
pub(crate) fn answers(url: &tauri::Url) -> bool {
    if !is_local(url) {
        return true;
    }
    let Some(port) = url.port_or_known_default() else {
        return true;
    };
    let wait = std::time::Duration::from_millis(300);
    [
        std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::net::SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port)),
    ]
    .iter()
    .any(|addr| std::net::TcpStream::connect_timeout(addr, wait).is_ok())
}

/// This machine: the names a dev server is opened by.
fn is_local(url: &tauri::Url) -> bool {
    matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    )
}

/// This machine, or a `*.localhost` name (which the browser resolves to it).
fn is_loopback(url: &tauri::Url) -> bool {
    is_local(url)
        || url
            .host_str()
            .is_some_and(|host| host.ends_with(".localhost"))
}

/// The same address with the other of http / https, for an address that names its port (without one the port would
/// change as well).
fn other_scheme(url: &tauri::Url) -> Option<tauri::Url> {
    url.port()?;
    let mut other = url.clone();
    other
        .set_scheme(if url.scheme() == "https" {
            "http"
        } else {
            "https"
        })
        .ok()?;
    Some(other)
}

/// Whether a local address really serves pages, and at which address: a port that accepts connections is not yet a
/// page (a dev server may still be compiling and reset what it is asked), and a server started with a certificate
/// answers at `https://` only (what a browser gets for `http://` there is an empty reply, shown as an error page).
/// So `GET /` is made, any HTTP answer counts, and when the address as given gets none the other scheme is tried;
/// the address that answered is returned. A server that took the request but is slow to answer (compiling) counts
/// as up: the browser waits for it as well. `None`: nothing serves yet. Other addresses are returned as they are.
pub(crate) async fn ready(url: &tauri::Url) -> Option<tauri::Url> {
    ready_within(url, std::time::Duration::from_secs(4)).await
}

async fn ready_within(url: &tauri::Url, wait: std::time::Duration) -> Option<tauri::Url> {
    if !matches!(url.scheme(), "http" | "https") || !is_local(url) {
        return Some(url.clone());
    }
    let listening = {
        let url = url.clone();
        tokio::task::spawn_blocking(move || answers(&url))
            .await
            .unwrap_or(false)
    };
    if !listening {
        return None;
    }
    let client = reqwest::Client::builder()
        .timeout(wait)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .danger_accept_invalid_certs(true) // a local dev certificate is self-signed
        .build()
        .ok()?;
    let mut slow = None;
    for candidate in std::iter::once(url.clone()).chain(other_scheme(url)) {
        match client.get(candidate.as_str()).send().await {
            Ok(_) => return Some(candidate),
            Err(error) if error.is_timeout() => {
                slow.get_or_insert(candidate);
            }
            Err(_) => {}
        }
    }
    slow
}

/// Pause before the `attempt`-th new try at a page that failed to load: 1 s, 2 s, 4 s.
fn backoff(attempt: u8) -> std::time::Duration {
    std::time::Duration::from_millis(1000 << u32::from(attempt.saturating_sub(1)).min(3))
}

/// The engine could not load tab `tab`'s page and showed its own error page. A server that is still starting may
/// have reset the request, so the page is asked for again a few times (and the address corrected when the other of
/// http / https is what answers); when that does not help, AgentDeck's own error page takes its place.
fn navigation_failed(app: &AppHandle, tab: &str) {
    let attempt = failures(|map| {
        let entry = map.entry(tab.to_string()).or_default();
        if entry.1 {
            return None;
        }
        entry.1 = true;
        entry.0 = entry.0.saturating_add(1);
        Some(entry.0)
    })
    .flatten();
    let Some(attempt) = attempt else { return };
    let (app, tab) = (app.clone(), tab.to_string());
    tauri::async_runtime::spawn(async move {
        if let (Ok(view), Some(target)) = (webview(&app, &tab), requested(&tab)) {
            let parsed = parse(&target).ok();
            let retried = if attempt <= RETRIES {
                tokio::time::sleep(backoff(attempt)).await;
                match &parsed {
                    Some(url) => ready(url).await,
                    None => None,
                }
            } else {
                None
            };
            match retried {
                Some(url) => {
                    remember(&tab, url.as_str());
                    let _ = view.navigate(url);
                }
                None => {
                    // Nothing listens on the port, or the retries are used up and it still would not load.
                    let down = parsed
                        .as_ref()
                        .is_some_and(|url| is_local(url) && !answers(url));
                    if let Ok(page) = error_page(&target, if down { "closed" } else { "failed" }) {
                        let _ = view.navigate(page);
                    }
                }
            }
        }
        failures(|map| {
            if let Some(entry) = map.get_mut(&tab) {
                entry.1 = false;
            }
        });
    });
}

/// Where a tab goes for `url`: the page itself, or AgentDeck's error page when nothing listens on its local port.
fn destination(tab: &str, url: &str) -> Result<tauri::Url, String> {
    let target = parse(url)?;
    remember(tab, target.as_str());
    failures(|map| map.remove(tab));
    if answers(&target) {
        Ok(target)
    } else {
        error_page(target.as_str(), "closed")
    }
}

/// The address without terminal colour codes: a server's log prints its port in colour (`\x1b[1m1431\x1b[22m`).
fn without_colours(text: &str) -> String {
    regex::Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]")
        .map_or_else(|_| text.to_string(), |codes| codes.replace_all(text, "").into_owned())
}

pub(crate) fn parse(url: &str) -> Result<tauri::Url, String> {
    // A bare host or "localhost:4200" is what people type; give it a scheme.
    let url = without_colours(url);
    let url = url.trim();
    let text = if url.contains("://") || url.starts_with("about:") || url.starts_with("data:") {
        url.to_owned()
    } else if url.starts_with("localhost") || url.starts_with("127.") || url.starts_with("[::1]") {
        format!("http://{url}")
    } else if url.contains(char::is_whitespace) || !url.contains('.') {
        // Words, not an address ("zz", "svelte runes"): search for them, as a browser's address bar does.
        return tauri::Url::parse_with_params("https://www.google.com/search", &[("q", url)])
            .map_err(|error| error.to_string());
    } else {
        format!("https://{url}")
    };
    text.parse().map_err(|error| format!("{error}"))
}

fn place(view: &Webview, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    set_rect(view, x, y, w, h)
}

/// Puts a child webview at a rectangle of the window (CSS pixels), in one step.
///
/// `set_position` then `set_size` are two moves of the page's host window, each queued (`SWP_ASYNCWINDOWPOS`) with
/// the page's own bounds set in between, half old, half new: processed out of order they left the page torn or
/// laid out at a size it no longer had, until the next resize. Here the host window moves and sizes at once, the
/// page gets the same size, and WebView2 is told it moved. Edges are rounded, not the size, so the page lines up
/// with the panel to the pixel.
pub(crate) fn set_rect(view: &Webview, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    #[cfg(windows)]
    {
        let scale = view
            .window()
            .scale_factor()
            .map_err(|error| error.to_string())?;
        let left = (x * scale).round() as i32;
        let top = (y * scale).round() as i32;
        let width = (((x + w) * scale).round() as i32 - left).max(1);
        let height = (((y + h) * scale).round() as i32 - top).max(1);
        view.with_webview(move |platform| {
            use windows::Win32::Foundation::{HWND, RECT};
            use windows::Win32::UI::WindowsAndMessaging::{
                SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
            };
            // SAFETY: WebView2 and Win32 calls on the thread that owns the webview, as `with_webview` runs us.
            unsafe {
                let controller = platform.controller();
                let mut host = HWND::default();
                if controller.ParentWindow(&mut host).is_ok() {
                    let _ = SetWindowPos(
                        host,
                        None,
                        left,
                        top,
                        width,
                        height,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                }
                let _ = controller.SetBounds(RECT {
                    left: 0,
                    top: 0,
                    right: width,
                    bottom: height,
                });
                let _ = controller.NotifyParentWindowPositionChanged();
            }
        })
        .map_err(|error| error.to_string())
    }
    #[cfg(not(windows))]
    {
        view.set_bounds(tauri::Rect {
            position: LogicalPosition::new(x, y).into(),
            size: LogicalSize::new(w.max(1.0), h.max(1.0)).into(),
        })
        .map_err(|error| error.to_string())
    }
}

/// Opens tab `tab` at `url` inside the given rectangle (window coordinates), or shows and moves it if it exists.
/// (async on purpose: creating a webview from a synchronous command deadlocks on Windows.)
#[tauri::command]
#[allow(clippy::too_many_arguments)] // one parameter per field the frontend sends
pub async fn browser_show(
    app: AppHandle,
    tab: String,
    url: String,
    user_agent: Option<String>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), String> {
    crate::bridge::set_active(&tab);
    let _creating = CREATING.lock().await;
    if let Ok(view) = webview(&app, &tab) {
        place(&view, x, y, w, h)?;
        view.show().map_err(|error| error.to_string())?;
        layer(&view, BELOW.load(Ordering::Relaxed));
        // The other tabs go only now: hiding them first left a frame with no page at all.
        hide_others(&app, &tab);
        return Ok(());
    }
    let window = app.get_window("main").ok_or("no main window")?;
    let mut builder =
        WebviewBuilder::new(label(&tab), WebviewUrl::External(destination(&tab, &url)?))
            .initialization_script(injected_script(&tab))
            .browser_extensions_enabled(EXTENSIONS_ENABLED)
            .on_page_load(|view, payload| {
                let event = match payload.event() {
                    PageLoadEvent::Started => "started",
                    PageLoadEvent::Finished => "finished",
                };
                let tab = view.label().trim_start_matches(PREFIX).to_string();
                let url = payload.url().to_string();
                // The browser's own error page (Chromium's chrome-error:// address) is replaced by AgentDeck's, at the same tab.
                let failed = url.starts_with("chrome-error:");
                // Our error page is served by the agentdeck scheme; the tab keeps showing the address that was asked for.
                let ours = url.starts_with(&format!("{}error", base()));
                if failed {
                    if event == "finished" {
                        navigation_failed(view.app_handle(), &tab);
                    }
                    return;
                }
                if event == "started" && !ours {
                    remember(&tab, &url);
                    crate::bridge::page_started(&tab, &url);
                }
                #[cfg(not(windows))]
                {
                    if !ours {
                        fallback::page_event(view.app_handle(), view.label(), event, &url);
                    }
                }
                let shown = if ours {
                    requested(&tab).unwrap_or_else(|| url.clone())
                } else {
                    url
                };
                let _ = view.app_handle().emit_to(
                    "main",
                    "browser-load",
                    serde_json::json!({
                        "label": view.label(),
                        "event": event,
                        "url": shown,
                    }),
                );
            });
    #[cfg(windows)]
    {
        let extension_root = app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?
            .join("browser-extensions");
        std::fs::create_dir_all(&extension_root).map_err(|error| error.to_string())?;
        builder = builder
            .extensions_path(extension_root);
    }
    // Opaque white under the page: while the page's window is resized (the mark-up bar or the developer tools open and
    // shrink the area) the strip not yet painted shows this, not black.
    builder = builder.background_color(tauri::window::Color(255, 255, 255, 255));
    // A phone or tablet identifies itself as one, so sites send their mobile layout.
    if let Some(agent) = user_agent.as_deref() {
        builder = builder.user_agent(agent);
    }
    let touch = user_agent.is_some();
    let view = window
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(w.max(1.0), h.max(1.0)),
        )
        .map_err(|error| error.to_string())?;
    // A phone or tablet takes touch, not a mouse: the pointer's presses and drags reach the page as touch events
    // (taps, swipes to scroll, `pointer: coarse`, no hover), the way a device's browser sends them.
    if touch {
        let view = view.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::cdp::call(
                &view,
                "Emulation.setTouchEmulationEnabled",
                serde_json::json!({ "enabled": true, "maxTouchPoints": 5 }),
                None,
            )
            .await;
            let _ = crate::cdp::call(
                &view,
                "Emulation.setEmitTouchEventsForMouse",
                serde_json::json!({ "enabled": true, "configuration": "mobile" }),
                None,
            )
            .await;
        });
    }
    allow_password_saving(&view);
    allow_local_certificates(&view);
    crate::cdp::watch_frames(&view);
    crate::cdp::watch_network(&view);
    // The page acts as focused and awake even when it is not the tab in front: an agent may drive it in the background.
    {
        let view = view.clone();
        tauri::async_runtime::spawn(async move { crate::cdp::prepare(&view).await });
    }
    layer(&view, BELOW.load(Ordering::Relaxed));
    hide_others(&app, &tab);
    Ok(())
}

/// Opens the system folder picker for an unpacked Chrome extension.
#[tauri::command]
pub async fn browser_pick_extension(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Select an unpacked browser extension")
            .blocking_pick_folder()
    })
    .await
    .map_err(|error| error.to_string())?;

    picked
        .map(|path| {
            path.into_path()
                .map(|path| path.display().to_string())
                .map_err(|error| error.to_string())
        })
        .transpose()
}

/// Copies an unpacked extension into AgentDeck's persistent extension folder and installs it in the Preview profile.
#[tauri::command]
pub async fn browser_add_extension(
    app: AppHandle,
    tab: String,
    path: String,
) -> Result<String, String> {
    #[cfg(not(windows))]
    {
        let _ = (app, tab, path);
        return Err("Browser extensions are currently supported only on Windows.".into());
    }

    #[cfg(windows)]
    {
        let source = PathBuf::from(path)
            .canonicalize()
            .map_err(|error| format!("Could not open the extension folder: {error}"))?;
        let manifest = read_extension_manifest(&source)?;
        let name = manifest
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .ok_or("The extension manifest must have a valid name.")?;

        let root = crate::browser_extensions::root(&app)?;
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let folder = extension_folder_name(&source);
        let destination = root.join(&folder);
        if !destination.exists() {
            if let Err(error) = copy_extension(&source, &destination) {
                let _ = std::fs::remove_dir_all(&destination);
                return Err(error);
            }
        }

        // No page open: the folder is in place, and the Preview loads it when its next tab opens (`extensions_path`).
        let Ok(view) = webview(&app, &tab) else {
            return Ok(name.to_owned());
        };
        let extension_path = destination.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        view.with_webview(move |platform| {
            use webview2_com::{take_pwstr, ProfileAddBrowserExtensionCompletedHandler};
            use windows::core::{HSTRING, PWSTR};

            let result = (|| unsafe {
                let profile = crate::browser_extensions::profile_of(&platform)?;
                let path = HSTRING::from(extension_path.as_path());
                let id = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
                let seen = std::rc::Rc::clone(&id);
                ProfileAddBrowserExtensionCompletedHandler::wait_for_async_operation(
                    Box::new(move |handler| {
                        profile
                            .AddBrowserExtension(&path, &handler)
                            .map_err(webview2_com::Error::WindowsError)
                    }),
                    Box::new(move |error, extension| {
                        error?;
                        if let Some(extension) = extension {
                            let mut raw = PWSTR::null();
                            extension.Id(&mut raw)?;
                            *seen.borrow_mut() = take_pwstr(raw);
                        }
                        Ok(())
                    }),
                )
                .map_err(|error| error.to_string())?;
                Ok::<String, String>(id.take())
            })();
            let _ = sender.send(result);
        })
        .map_err(|error| error.to_string())?;
        let id = receiver.await.map_err(|error| error.to_string())??;
        // The profile knows it by id, the folder by name: kept so the menu can show its toolbar page and remove its folder.
        if !id.is_empty() {
            crate::browser_extensions::remember_id(&root, &id, &folder)?;
        }

        Ok(name.to_owned())
    }
}

#[cfg(windows)]
fn read_extension_manifest(path: &Path) -> Result<serde_json::Value, String> {
    let manifest_path = path.join("manifest.json");
    let text = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("Could not read manifest.json: {error}"))?;
    let manifest: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| format!("manifest.json is not valid JSON: {error}"))?;
    if !manifest
        .get("manifest_version")
        .is_some_and(|version| matches!(version.as_u64(), Some(2 | 3)))
    {
        return Err("The extension must use manifest version 2 or 3.".into());
    }
    if manifest
        .get("name")
        .and_then(serde_json::Value::as_str)
        .is_none()
    {
        return Err("The extension manifest must include a name.".into());
    }
    Ok(manifest)
}

#[cfg(windows)]
fn extension_folder_name(source: &Path) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(source.to_string_lossy().as_bytes());
    let prefix = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("extension-{prefix}")
}

#[cfg(windows)]
fn copy_extension(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_symlink() {
            return Err("The extension folder cannot contain symbolic links.".into());
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::copy(entry.path(), &target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Takes a page off the user's screen without hiding it: it is moved far off-screen but stays shown, so it keeps
/// drawing. A hidden page stops rendering and a screenshot of it would hang; this way an agent that owns the tab
/// can still look at it and act on it while another tab is in front.
fn stash(view: &Webview) {
    let scale = view.window().scale_factor().unwrap_or(1.0);
    if let Ok(size) = view.size() {
        let size = size.to_logical::<f64>(scale);
        let _ = set_rect(view, -20000.0, -20000.0, size.width, size.height);
    } else {
        let _ = view.hide();
    }
}

/// Takes every page but tab `tab`'s off the user's screen (see `stash`).
fn hide_others(app: &AppHandle, tab: &str) {
    let keep = label(tab);
    for (name, view) in app.webviews() {
        if name.starts_with(PREFIX) && name != keep {
            stash(&view);
        }
    }
}

/// Whether the pages are below the app (something of the app is open over the page).
static BELOW: AtomicBool = AtomicBool::new(false);

/// Puts a page above the app's own webview (the page takes the pointer and the keyboard) or below it.
///
/// The page is a native window: nothing of the app can be drawn over it. The app is see-through where the page is,
/// so with the page below, the app draws its modal, menu or tooltip over the live page like anywhere else. Only
/// the stacking order changes: instant, and the page keeps drawing.
fn layer(view: &Webview, below: bool) {
    #[cfg(windows)]
    let _ = view.with_webview(move |platform| {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_BOTTOM, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        };
        // SAFETY: plain Win32 calls on the window that hosts this webview, on its own thread.
        unsafe {
            let mut host = HWND::default();
            if platform.controller().ParentWindow(&mut host).is_ok() {
                let order = if below { HWND_BOTTOM } else { HWND_TOP };
                let _ = SetWindowPos(
                    host,
                    Some(order),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    });
    #[cfg(not(windows))]
    let _ = (view, below);
}

/// Puts every page below the app (`below`) or back above it.
#[tauri::command]
pub fn browser_layer(app: AppHandle, below: bool) {
    BELOW.store(below, Ordering::Relaxed);
    for (name, view) in app.webviews() {
        if name.starts_with(PREFIX) {
            layer(&view, below);
        }
    }
}

/// The page's own password manager: the browser offers to save a password after a sign-in and fills it next time,
/// as Edge or Chrome do. The setting belongs to the WebView2 profile, so it covers every page.
#[cfg(windows)]
fn allow_password_saving(view: &Webview) {
    let _ = view.with_webview(|platform| {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings4;
        use windows::core::Interface;
        // SAFETY: WebView2 calls on the thread that owns the webview, as `with_webview` runs us.
        unsafe {
            let Ok(core) = platform.controller().CoreWebView2() else {
                return;
            };
            let Ok(settings) = core.Settings() else {
                return;
            };
            if let Ok(settings) = settings.cast::<ICoreWebView2Settings4>() {
                let _ = settings.SetIsPasswordAutosaveEnabled(true);
                let _ = settings.SetIsGeneralAutofillEnabled(true);
            }
        }
    });
}

#[cfg(not(windows))]
fn allow_password_saving(_view: &Webview) {}

/// A dev server started with `--ssl` has a self-signed certificate, which the browser would stop at with a warning
/// page (the engine's own error page, where this app cannot help). For this machine's own addresses (localhost,
/// 127.0.0.1, [::1], `*.localhost`) the certificate is accepted; every other site keeps the browser's check.
#[cfg(windows)]
fn allow_local_certificates(view: &Webview) {
    let _ = view.with_webview(|platform| {
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            ICoreWebView2_14, COREWEBVIEW2_SERVER_CERTIFICATE_ERROR_ACTION_ALWAYS_ALLOW,
        };
        use webview2_com::ServerCertificateErrorDetectedEventHandler;
        use windows::core::{Interface, PWSTR};
        // SAFETY: WebView2 calls on the thread that owns the webview, as `with_webview` runs us.
        unsafe {
            let Ok(core) = platform.controller().CoreWebView2() else {
                return;
            };
            let Ok(core) = core.cast::<ICoreWebView2_14>() else {
                return;
            };
            let handler =
                ServerCertificateErrorDetectedEventHandler::create(Box::new(|_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut uri = PWSTR::null();
                    args.RequestUri(&mut uri)?;
                    let uri = webview2_com::take_pwstr(uri);
                    if tauri::Url::parse(&uri).is_ok_and(|url| is_loopback(&url)) {
                        args.SetAction(COREWEBVIEW2_SERVER_CERTIFICATE_ERROR_ACTION_ALWAYS_ALLOW)?;
                    }
                    Ok(())
                }));
            let mut token = 0i64;
            let _ = core.add_ServerCertificateErrorDetected(&handler, &mut token);
        }
    });
}

#[cfg(not(windows))]
fn allow_local_certificates(_view: &Webview) {}

/// A real press at a point of the page, as a person's mouse would make it. `x` and `y` are CSS pixels of the page
/// and `zoom` is the page's zoom (1 at 100%).
#[tauri::command]
pub fn browser_press(app: AppHandle, tab: String, x: f64, y: f64, zoom: f64) -> Result<(), String> {
    let view = webview(&app, &tab)?;
    let window = app.get_window("main").ok_or("no main window")?;
    let scale = window.scale_factor().map_err(|error| error.to_string())?;
    let origin = window.inner_position().map_err(|error| error.to_string())?;
    let at = view.position().map_err(|error| error.to_string())?;
    // Physical pixels: the window's client area, then the page inside it, then the point.
    let px = origin.x as f64 + at.x as f64 + x * zoom * scale;
    let py = origin.y as f64 + at.y as f64 + y * zoom * scale;
    #[cfg(windows)]
    {
        let hwnd = window.hwnd().map_err(|error| error.to_string())?;
        press_at(hwnd.0 as isize, px.round() as i32, py.round() as i32);
    }
    #[cfg(not(windows))]
    {
        // macOS wants points, X11 pixels.
        let divide = if cfg!(target_os = "macos") {
            scale
        } else {
            1.0
        };
        press_at(
            0,
            (px / divide).round() as i32,
            (py / divide).round() as i32,
        );
    }
    Ok(())
}

#[cfg(windows)]
fn press_at(hwnd: isize, x: i32, y: i32) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        mouse_event, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    };
    use windows::Win32::UI::WindowsAndMessaging::{SetCursorPos, SetForegroundWindow};
    // SAFETY: plain Win32 input calls, for the app's own window.
    unsafe {
        let _ = SetForegroundWindow(HWND(hwnd as *mut _));
        let _ = SetCursorPos(x, y);
        std::thread::sleep(std::time::Duration::from_millis(30));
        mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
        std::thread::sleep(std::time::Duration::from_millis(40));
        mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
    }
}

#[cfg(not(windows))]
fn press_at(_hwnd: isize, x: i32, y: i32) {
    use enigo::{Button, Coordinate, Direction, Enigo, Mouse, Settings};
    // macOS asks for the Accessibility permission the first time; Wayland sessions cannot be driven this way.
    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return;
    };
    let _ = enigo.move_mouse(x, y, Coordinate::Abs);
    std::thread::sleep(std::time::Duration::from_millis(30));
    let _ = enigo.button(Button::Left, Direction::Press);
    std::thread::sleep(std::time::Duration::from_millis(40));
    let _ = enigo.button(Button::Left, Direction::Release);
}

/// Follows the panel when it moves or resizes.
#[tauri::command]
pub fn browser_bounds(
    app: AppHandle,
    tab: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), String> {
    crate::bridge::set_active(&tab);
    place(&webview(&app, &tab)?, x, y, w, h)
}

/// Takes every tab's page off the screen (a menu or the annotation view needs the space, or another tab comes
/// forward). The pages stay shown off-screen, so an agent can still act on them (see `stash`).
#[tauri::command]
pub fn browser_hide(app: AppHandle) -> Result<(), String> {
    for (name, view) in app.webviews() {
        if name.starts_with(PREFIX) {
            stash(&view);
        }
    }
    Ok(())
}

/// The Preview panel is not on screen (another tool is open): the front page is moved far off-screen but stays
/// shown, so it keeps drawing. A hidden page stops rendering, and the agent's screenshots of it would hang; this
/// way it can still look at the page and act on it. The other tabs are hidden as usual.
#[tauri::command]
pub fn browser_park(app: AppHandle) -> Result<(), String> {
    let front = format!("{PREFIX}{}", crate::bridge::active_tab());
    for (name, view) in app.webviews() {
        if !name.starts_with(PREFIX) {
            continue;
        }
        if name == front {
            let size = view
                .size()
                .map_err(|error| error.to_string())?
                .to_logical::<f64>(
                    view.window()
                        .scale_factor()
                        .map_err(|error| error.to_string())?,
                );
            set_rect(&view, -20000.0, -20000.0, size.width, size.height)?;
            view.show().map_err(|error| error.to_string())?;
        } else {
            view.hide().map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// The app window was reloaded (F5): the pages opened before it belong to tabs that no longer exist, yet they stay
/// on screen above the new window. Close them all.
#[tauri::command]
pub fn browser_reset(app: AppHandle) {
    for (name, view) in app.webviews() {
        if name.starts_with(PREFIX) {
            let _ = view.close();
        }
    }
    crate::bridge::set_active("");
}

#[tauri::command]
pub fn browser_navigate(app: AppHandle, tab: String, url: String) -> Result<(), String> {
    webview(&app, &tab)?
        .navigate(destination(&tab, &url)?)
        .map_err(|error| error.to_string())
}

/// Runs a snippet in a tab's page: back, forward, reload, or a request to the injected script.
#[tauri::command]
pub fn browser_eval(app: AppHandle, tab: String, script: String) -> Result<(), String> {
    webview(&app, &tab)?
        .eval(script)
        .map_err(|error| error.to_string())
}

/// Page zoom, used to fit a phone or tablet width into the panel.
#[tauri::command]
pub fn browser_zoom(app: AppHandle, tab: String, factor: f64) -> Result<(), String> {
    webview(&app, &tab)?
        .set_zoom(factor.clamp(0.25, 3.0))
        .map_err(|error| error.to_string())
}

/// A picture of what a tab shows now, taken by the browser engine itself (works on any page, whatever its
/// policies), as a PNG data URL. The app shows it in place of the page while a menu is open over it.
#[tauri::command]
pub async fn browser_capture(
    app: AppHandle,
    tab: String,
    scale: Option<f64>,
    full_page: Option<bool>,
) -> Result<String, String> {
    let bytes = capture_png(&app, &tab, scale, full_page.unwrap_or(false)).await?;
    use base64::Engine;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// The same picture as `browser_capture`, saved straight to a WebP file (see picture_store.rs): only its path goes back,
/// so no base64 crosses to the page. Returns the path under `~/.agentdeck/attachments`.
#[tauri::command]
pub async fn browser_capture_file(
    app: AppHandle,
    tab: String,
    scale: Option<f64>,
    full_page: Option<bool>,
) -> Result<String, String> {
    let bytes = capture_png(&app, &tab, scale, full_page.unwrap_or(false)).await?;
    crate::save_attachment_bytes("image/png", bytes).await
}

/// A picture of one element of a tab, cut from the page at its box (CSS pixels), as a PNG data URL. Only the
/// element is sent to the chat: the agent gets this picture next to the element's markup.
#[tauri::command]
pub async fn browser_element_capture(
    app: AppHandle,
    tab: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<String, String> {
    use base64::Engine;
    let bytes = element_png(&app, &tab, x, y, w, h).await?;
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

/// `browser_element_capture`, saved to a WebP file: returns its path.
#[tauri::command]
pub async fn browser_element_capture_file(
    app: AppHandle,
    tab: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<String, String> {
    let bytes = element_png(&app, &tab, x, y, w, h).await?;
    crate::save_attachment_bytes("image/png", bytes).await
}

/// The PNG bytes of a tab: the whole page (`full_page`), a scaled capture (`scale` above 1), or the visible page.
async fn capture_png(app: &AppHandle, tab: &str, scale: Option<f64>, full_page: bool) -> Result<Vec<u8>, String> {
    let view = webview(app, tab)?;
    if full_page {
        return crate::cdp::screenshot_full_page(&view).await;
    }
    if scale.unwrap_or(1.0) > 1.0 {
        return crate::cdp::screenshot_scaled(&view, scale.unwrap_or(1.0)).await;
    }
    let (sender, receiver) = tokio::sync::oneshot::channel::<Result<Vec<u8>, String>>();
    capture(&view, sender)?;
    receiver
        .await
        .map_err(|_| "the capture was dropped".to_string())?
}

async fn element_png(app: &AppHandle, tab: &str, x: f64, y: f64, w: f64, h: f64) -> Result<Vec<u8>, String> {
    let view = webview(app, tab)?;
    crate::cdp::screenshot_clip(&view, x, y, w.max(1.0), h.max(1.0)).await
}

#[cfg(windows)]
fn capture(
    view: &Webview,
    sender: tokio::sync::oneshot::Sender<Result<Vec<u8>, String>>,
) -> Result<(), String> {
    use webview2_com::CapturePreviewCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG;
    use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
    use windows::Win32::System::Com::{IStream, STREAM_SEEK_SET};

    view.with_webview(move |platform| {
        let started = (|| -> Result<(), String> {
            // SAFETY: WebView2 COM calls on the thread that owns the webview, as `with_webview` runs us.
            unsafe {
                let core = platform
                    .controller()
                    .CoreWebView2()
                    .map_err(|e| e.to_string())?;
                let stream: IStream =
                    CreateStreamOnHGlobal(Default::default(), true).map_err(|e| e.to_string())?;
                let reader = stream.clone();
                let mut sender = Some(sender);
                let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
                    let bytes = (|| -> Result<Vec<u8>, String> {
                        result.map_err(|e| e.to_string())?;
                        reader
                            .Seek(0, STREAM_SEEK_SET, None)
                            .map_err(|e| e.to_string())?;
                        let mut out = Vec::new();
                        let mut chunk = vec![0u8; 64 * 1024];
                        loop {
                            let mut read = 0u32;
                            let _ = reader.Read(
                                chunk.as_mut_ptr().cast(),
                                chunk.len() as u32,
                                Some(&mut read),
                            );
                            if read == 0 {
                                break;
                            }
                            out.extend_from_slice(&chunk[..read as usize]);
                        }
                        Ok(out)
                    })();
                    if let Some(sender) = sender.take() {
                        let _ = sender.send(bytes);
                    }
                    Ok(())
                }));
                core.CapturePreview(
                    COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
                    &stream,
                    &handler,
                )
                .map_err(|e| e.to_string())
            }
        })();
        if let Err(error) = started {
            eprintln!("capture failed: {error}");
        }
    })
    .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn capture(
    _view: &Webview,
    _sender: tokio::sync::oneshot::Sender<Result<Vec<u8>, String>>,
) -> Result<(), String> {
    Err("not supported on this platform".into())
}

#[tauri::command]
pub fn browser_close(app: AppHandle, tab: String) -> Result<(), String> {
    match webview(&app, &tab) {
        Ok(view) => view.close().map_err(|error| error.to_string()),
        Err(_) => Ok(()),
    }
}

/// Opens a web address in the system browser. Only http(s) addresses.
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("only http and https addresses can be opened".into());
    }
    let mut command = if cfg!(windows) {
        let mut command = std::process::Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", &url]);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = std::process::Command::new("open");
        command.arg(&url);
        command
    } else {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(&url);
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Developer check (set `AGENTDECK_BROWSER_SELFTEST=<url>`): opens a tab on a page, asks it for its DOM and
/// a screenshot and prints what the injected script reports, then quits. It proves the whole round trip.
pub fn selftest(app: AppHandle, url: String) {
    use std::time::Duration;
    use tauri::Listener;
    app.listen("browser-report", |event| {
        let payload = event.payload();
        let shown: String = payload.chars().take(220).collect();
        eprintln!("report ({} bytes): {shown}", payload.len());
    });
    app.listen("browser-load", |event| {
        eprintln!("load: {}", event.payload())
    });
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        eprintln!(
            "show: {:?}",
            browser_show(
                app.clone(),
                "t1".into(),
                url,
                None,
                280.0,
                60.0,
                600.0,
                500.0
            )
            .await
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
        for (id, kind) in [("a", "dom"), ("b", "shot")] {
            eprintln!(
                "eval {kind}: {:?}",
                browser_eval(
                    app.clone(),
                    "t1".into(),
                    format!("window.__agentdeck.run('{id}','{kind}')")
                )
            );
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
        app.exit(0);
    });
}

/// A DevTools call for the developer tools of the app: response bodies, cookies, storage, IndexedDB, caches. Only the
/// domains those panels need; the rest of the protocol stays with the agent's tools.
#[tauri::command]
pub async fn devtools_cdp(
    app: AppHandle,
    tab: String,
    method: String,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    const ALLOWED: [&str; 5] = [
        "Network.",
        "DOMStorage.",
        "IndexedDB.",
        "Storage.",
        "CacheStorage.",
    ];
    // IndexedDB hands back remote objects; turning one into text is a call on it (and letting go of them after).
    const EXACT: [&str; 2] = ["Runtime.callFunctionOn", "Runtime.releaseObjectGroup"];
    if !ALLOWED.iter().any(|prefix| method.starts_with(prefix)) && !EXACT.contains(&method.as_str())
    {
        return Err(format!("{method} is not available to the developer tools"));
    }
    let view = webview(&app, &tab)?;
    if method == "Network.clearBrowserCache" {
        crate::cdp::clear_cache(&view).await?;
        return Ok(serde_json::json!({}));
    }
    let params = params.unwrap_or_else(|| serde_json::json!({}));
    #[cfg(not(windows))]
    {
        if crate::cdp::call(&view, &method, params.clone(), None)
            .await
            .is_err()
        {
            return fallback::devtools(&app, &tab, &method, &params).await;
        }
    }
    crate::cdp::call(&view, &method, params, None).await
}

/// The developer tools without DevTools access (macOS, Linux): the Network tab is built from what the injected
/// script sees of fetch and XHR plus the page loads, and storage and cookies are read by the page itself.
#[cfg(not(windows))]
mod fallback {
    use std::collections::{HashMap, VecDeque};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{LazyLock, Mutex};

    use serde_json::{json, Value};
    use tauri::{AppHandle, Emitter};

    static NEXT: AtomicU64 = AtomicU64::new(1);
    /// Response bodies by (tab, request id), newest last, so the Network tab can show a response afterwards.
    static BODIES: LazyLock<Mutex<(HashMap<String, String>, VecDeque<String>)>> =
        LazyLock::new(Default::default);
    /// The document request of each page that is loading, by webview label.
    static DOCUMENTS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

    fn now() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|time| time.as_secs_f64())
            .unwrap_or_default()
    }

    fn emit(app: &AppHandle, label: &str, event: &str, params: Value) {
        let _ = app.emit(
            "devtools-net",
            json!({ "label": label, "event": event, "params": params }),
        );
    }

    /// A page load as the Document request that starts the list again, and its end.
    pub fn page_event(app: &AppHandle, label: &str, event: &str, url: &str) {
        if event == "started" {
            let id = format!("doc-{}", NEXT.fetch_add(1, Ordering::Relaxed));
            if let Ok(mut documents) = DOCUMENTS.lock() {
                documents.insert(label.to_string(), id.clone());
            }
            emit(
                app,
                label,
                "Network.requestWillBeSent",
                json!({
                    "requestId": id, "loaderId": id, "type": "Document", "timestamp": now(), "wallTime": now(),
                    "request": { "url": url, "method": "GET", "headers": {} },
                }),
            );
        } else if let Some(id) = DOCUMENTS
            .lock()
            .ok()
            .and_then(|mut documents| documents.remove(label))
        {
            emit(
                app,
                label,
                "Network.responseReceived",
                json!({
                    "requestId": id, "type": "Document", "timestamp": now(),
                    "response": { "url": url, "status": 200, "statusText": "", "mimeType": "text/html", "headers": {} },
                }),
            );
            emit(
                app,
                label,
                "Network.loadingFinished",
                json!({ "requestId": id, "timestamp": now(), "encodedDataLength": 0 }),
            );
        }
    }

    /// A fetch or XHR the page reported, as the three events the Network tab is built from.
    pub fn network_events(app: &AppHandle, report: &Value) {
        let tab = report["tab"].as_str().unwrap_or_default();
        let label = format!("browser-{tab}");
        let id = format!("req-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let ended = report["at"]
            .as_f64()
            .map_or_else(now, |millis| millis / 1000.0);
        let started = ended - report["ms"].as_f64().unwrap_or_default() / 1000.0;
        let status = report["status"].as_i64().unwrap_or_default();
        let kind = report["via"].as_str().unwrap_or("Fetch");
        let mut request =
            json!({ "url": report["url"], "method": report["method"], "headers": {} });
        if let Some(sent) = report["request"].as_str() {
            request["postData"] = json!(sent);
        }
        emit(
            app,
            &label,
            "Network.requestWillBeSent",
            json!({ "requestId": id, "loaderId": "", "type": kind, "timestamp": started, "wallTime": started, "request": request }),
        );
        if status == 0 {
            emit(
                app,
                &label,
                "Network.loadingFailed",
                json!({ "requestId": id, "timestamp": ended, "errorText": "failed" }),
            );
            return;
        }
        let mime = if report["body"]
            .as_str()
            .is_some_and(|body| body.trim_start().starts_with(['{', '[']))
        {
            "application/json"
        } else {
            "text/plain"
        };
        emit(
            app,
            &label,
            "Network.responseReceived",
            json!({
                "requestId": id, "type": kind, "timestamp": ended,
                "response": { "url": report["url"], "status": status, "statusText": "", "mimeType": mime, "headers": {} },
            }),
        );
        let body = report["body"].as_str().unwrap_or_default();
        emit(
            app,
            &label,
            "Network.loadingFinished",
            json!({ "requestId": id, "timestamp": ended, "encodedDataLength": body.len() }),
        );
        if !body.is_empty() {
            if let Ok(mut bodies) = BODIES.lock() {
                let key = format!("{tab}|{id}");
                bodies.0.insert(key.clone(), body.to_string());
                bodies.1.push_back(key);
                while bodies.1.len() > 300 {
                    if let Some(old) = bodies.1.pop_front() {
                        bodies.0.remove(&old);
                    }
                }
            }
        }
    }

    /// Runs a script in the page and returns its value (the script returns JSON text).
    async fn page_json(app: &AppHandle, tab: &str, script: &str) -> Result<Value, String> {
        let report = crate::bridge::ask(app, tab, "eval", json!(script)).await?;
        if let Some(error) = report["error"].as_str() {
            return Err(error.to_string());
        }
        let text = report["result"].as_str().unwrap_or("null");
        serde_json::from_str(text).map_err(|error| error.to_string())
    }

    pub async fn devtools(
        app: &AppHandle,
        tab: &str,
        method: &str,
        params: &Value,
    ) -> Result<Value, String> {
        let local = params["storageId"]["isLocalStorage"]
            .as_bool()
            .unwrap_or(true);
        let store = if local {
            "localStorage"
        } else {
            "sessionStorage"
        };
        match method {
            m if m.ends_with(".enable") => Ok(json!({})),
            "Network.getResponseBody" => {
                let key = format!("{tab}|{}", params["requestId"].as_str().unwrap_or_default());
                let body = BODIES
                    .lock()
                    .ok()
                    .and_then(|bodies| bodies.0.get(&key).cloned());
                body.map(|body| json!({ "body": body, "base64Encoded": false }))
                    .ok_or_else(|| "the body of this response was not kept".to_string())
            }
            "DOMStorage.getDOMStorageItems" => {
                let entries = page_json(
                    app,
                    tab,
                    &format!("JSON.stringify(Object.entries({store}))"),
                )
                .await?;
                Ok(json!({ "entries": entries }))
            }
            "DOMStorage.removeDOMStorageItem" => {
                let key =
                    serde_json::to_string(&params["key"]).map_err(|error| error.to_string())?;
                page_json(app, tab, &format!("{store}.removeItem({key}); 'null'")).await?;
                Ok(json!({}))
            }
            "DOMStorage.clear" => {
                page_json(app, tab, &format!("{store}.clear(); 'null'")).await?;
                Ok(json!({}))
            }
            "Network.getCookies" => {
                let script = "JSON.stringify(document.cookie.split('; ').filter(Boolean).map((item) => { const at = item.indexOf('='); return { name: item.slice(0, at), value: item.slice(at + 1), domain: location.hostname, path: '/', expires: -1, httpOnly: false, secure: location.protocol === 'https:' }; }))";
                Ok(json!({ "cookies": page_json(app, tab, script).await? }))
            }
            "Network.deleteCookies" => {
                let name =
                    serde_json::to_string(&params["name"]).map_err(|error| error.to_string())?;
                let path =
                    serde_json::to_string(&params["path"]).map_err(|error| error.to_string())?;
                let script = format!("document.cookie = {name} + '=; expires=Thu, 01 Jan 1970 00:00:00 GMT; path=' + ({path} || '/'); 'null'");
                page_json(app, tab, &script).await?;
                Ok(json!({}))
            }
            "Storage.clearDataForOrigin" => {
                page_json(
                    app,
                    tab,
                    "localStorage.clear(); sessionStorage.clear(); 'null'",
                )
                .await?;
                Ok(json!({}))
            }
            "Network.clearBrowserCache" => Ok(json!({})),
            _ => Err(format!("{method} is not available on this platform")),
        }
    }
}

/// The icon of a page as a data URL, for the tab: the page's own `<link rel=icon>` when it named one, else the site's
/// `/favicon.ico`. The app's content policy does not let it show images straight from a site, so it is fetched here.
#[tauri::command]
pub async fn browser_favicon(page: String, icon: Option<String>) -> Result<String, String> {
    use base64::Engine;
    let base = reqwest::Url::parse(&page).map_err(|error| error.to_string())?;
    if !matches!(base.scheme(), "http" | "https") {
        return Err("not a web page".into());
    }
    let mut candidates: Vec<reqwest::Url> = Vec::new();
    if let Some(href) = icon.filter(|href| !href.is_empty()) {
        if let Ok(url) = base.join(&href) {
            candidates.push(url);
        }
    }
    if let Ok(url) = base.join("/favicon.ico") {
        if !candidates.contains(&url) {
            candidates.push(url);
        }
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .user_agent("Mozilla/5.0 AgentDeck")
        .build()
        .map_err(|error| error.to_string())?;
    for url in candidates {
        if !matches!(url.scheme(), "http" | "https") {
            continue;
        }
        let Ok(response) = client.get(url).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let kind = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if kind.starts_with("text/") || kind.contains("json") {
            continue;
        }
        let Ok(bytes) = response.bytes().await else {
            continue;
        };
        if bytes.is_empty() || bytes.len() > 262_144 {
            continue;
        }
        let mime = if kind.starts_with("image/") {
            kind
        } else {
            "image/x-icon".to_string()
        };
        return Ok(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ));
    }
    Err("no icon".into())
}

/// What Google suggests for a search being typed in the address bar (the first few phrases; none when offline).
#[tauri::command]
pub async fn browser_suggest(query: String) -> Vec<String> {
    let query = query.trim();
    if query.is_empty() || query.len() > 200 {
        return Vec::new();
    }
    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    else {
        return Vec::new();
    };
    let Ok(mut url) =
        reqwest::Url::parse("https://suggestqueries.google.com/complete/search?client=firefox")
    else {
        return Vec::new();
    };
    url.query_pairs_mut().append_pair("q", query);
    let Ok(response) = client.get(url).send().await else {
        return Vec::new();
    };
    let Ok(value) = response.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    value
        .get(1)
        .and_then(|list| list.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .take(5)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// A plain HTTP server on a free local port that answers every request with `status`; `None` takes the request
    /// and never answers.
    async fn server(status: Option<u16>) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("binds");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buffer = [0u8; 1024];
                    let _ = stream.read(&mut buffer).await;
                    match status {
                        Some(code) => {
                            let _ = stream.write_all(format!("HTTP/1.1 {code} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await;
                        }
                        None => tokio::time::sleep(Duration::from_secs(30)).await,
                    }
                });
            }
        });
        port
    }

    fn url(scheme: &str, port: u16) -> tauri::Url {
        format!("{scheme}://localhost:{port}/")
            .parse()
            .expect("url")
    }

    #[tokio::test]
    async fn a_port_with_no_server_is_not_ready() {
        let free = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("binds")
            .local_addr()
            .expect("address")
            .port();
        assert_eq!(
            ready_within(&url("http", free), Duration::from_millis(500)).await,
            None
        );
    }

    #[tokio::test]
    async fn any_http_answer_makes_a_server_ready_at_its_own_address() {
        for status in [200, 404, 500] {
            let port = server(Some(status)).await;
            let wanted = url("http", port);
            assert_eq!(
                ready_within(&wanted, Duration::from_millis(1500)).await,
                Some(wanted),
                "status {status}"
            );
        }
    }

    #[tokio::test]
    async fn the_scheme_the_server_speaks_is_the_one_returned() {
        // A plain server asked for as https (what an address with the wrong scheme looks like): the http address answers.
        let port = server(Some(200)).await;
        assert_eq!(
            ready_within(&url("https", port), Duration::from_millis(1500)).await,
            Some(url("http", port))
        );
    }

    #[tokio::test]
    async fn a_server_that_resets_connections_is_not_ready() {
        // Like a dev server that has the port but is not serving yet: it accepts and closes.
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("binds");
        let port = listener.local_addr().expect("address").port();
        tokio::spawn(async move { while listener.accept().await.is_ok() {} });
        assert_eq!(
            ready_within(&url("http", port), Duration::from_millis(500)).await,
            None
        );
    }

    #[tokio::test]
    async fn a_server_that_is_slow_to_answer_counts_as_up() {
        // A compiling dev server holds the request: the browser would wait too, so the address is kept as it is.
        let port = server(None).await;
        let wanted = url("http", port);
        assert_eq!(
            ready_within(&wanted, Duration::from_millis(300)).await,
            Some(wanted)
        );
    }

    #[test]
    fn colour_codes_from_a_log_are_not_part_of_the_address() {
        let coloured = "http://127.0.0.1:\u{1b}[1m1431\u{1b}[22m/\u{1b}[39m";
        assert_eq!(parse(coloured).expect("url").as_str(), "http://127.0.0.1:1431/");
        assert_eq!(parse("  localhost:4200 ").expect("url").as_str(), "http://localhost:4200/");
    }

    #[test]
    fn only_this_machine_is_probed_and_trusted() {
        let parsed = |text: &str| -> tauri::Url { text.parse().expect("url") };
        assert!(is_local(&parsed("http://localhost:4205/")));
        assert!(is_local(&parsed("https://127.0.0.1:3000")));
        assert!(is_local(&parsed("http://[::1]:5173/")));
        assert!(!is_local(&parsed("https://example.com/")));
        assert!(is_loopback(&parsed("https://vite.localhost:4205/")));
        assert!(!is_loopback(&parsed("https://notlocalhost.example/")));
        assert!(!is_loopback(&parsed("https://localhost.evil.example/")));
    }

    #[test]
    fn the_other_scheme_keeps_the_port_and_needs_one() {
        let parsed = |text: &str| -> tauri::Url { text.parse().expect("url") };
        assert_eq!(
            other_scheme(&parsed("http://localhost:4205/")),
            Some(parsed("https://localhost:4205/"))
        );
        assert_eq!(
            other_scheme(&parsed("https://localhost:4205/x?y=1")),
            Some(parsed("http://localhost:4205/x?y=1"))
        );
        assert_eq!(
            other_scheme(&parsed("http://localhost/")),
            None,
            "without a port the other scheme is another port"
        );
    }

    #[test]
    fn retries_wait_longer_each_time() {
        let waits: Vec<u64> = (1..=RETRIES)
            .map(|attempt| backoff(attempt).as_millis() as u64)
            .collect();
        assert_eq!(waits, [1000, 2000, 4000]);
    }
}
