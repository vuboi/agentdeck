//! The AgentDeck browser: one child webview per tab, laid over the preview panel.
//!
//! Each page runs in its own webview, so a tab behaves like a normal browser tab. A script injected into every
//! page (browser/inject.js) reports console, network and navigation, and answers requests for the DOM, a
//! screenshot and the element picker. Reports come back through the `agentdeck` URL scheme and are forwarded
//! to the app window as `browser-report` events; page loads are forwarded as `browser-load` events.

use tauri::webview::PageLoadEvent;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl,
};

const PREFIX: &str = "browser-";

/// Opening can be asked for twice at once (resize and navigation); only one may create a webview.
static CREATING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The address each tab asked for, by tab id. A failed load shows the tab's own error page at this address.
static REQUESTED: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);

/// Marks the error page AgentDeck shows in place of the browser's own (Chromium's `chrome-error://` page).
const ERROR_MARK: &str = "<!--agentdeck-error-->";

fn requested(tab: &str) -> Option<String> {
    REQUESTED.lock().ok()?.as_ref()?.get(tab).cloned()
}

fn remember(tab: &str, url: &str) {
    if let Ok(mut map) = REQUESTED.lock() {
        map.get_or_insert_with(Default::default).insert(tab.to_string(), url.to_string());
    }
}

/// AgentDeck's page for an address that did not answer, like a browser's own: the tab keeps the address, Reload
/// tries again, and the page opens the address by itself as soon as its server answers. It is served by the
/// `agentdeck` scheme (WebView2 does not show `data:` pages here).
pub(crate) fn error_html(target: &str) -> String {
    let shown = target.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;");
    let host = tauri::Url::parse(target)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| shown.clone());
    let go = target.replace('\\', "\\\\").replace('\'', "\\'").replace('"', "%22");
    format!(
        "{ERROR_MARK}<!doctype html><html><meta charset=utf-8><meta name=viewport content='width=device-width,initial-scale=1'><title>{host}</title><style>:root{{color-scheme:light dark;--bg:#ffffff;--fg:#202124;--muted:#5f6368;--accent:#1a73e8}}@media (prefers-color-scheme:dark){{:root{{--bg:#202124;--fg:#e8eaed;--muted:#9aa0a6;--accent:#8ab4f8}}}}body{{margin:0;min-height:100vh;background:var(--bg);color:var(--muted);font:15px/1.55 system-ui,'Segoe UI',sans-serif}}main{{max-width:560px;margin:0 auto;padding:18vh 32px 32px}}svg{{width:40px;height:40px;color:var(--accent)}}h1{{margin:24px 0 12px;font-size:22px;font-weight:500;color:var(--fg)}}ul{{margin:8px 0 0;padding-left:22px}}code{{font-size:12px;letter-spacing:.02em}}button{{margin-top:32px;padding:8px 18px;border:0;border-radius:6px;background:var(--accent);color:var(--bg);font:500 14px system-ui,sans-serif;cursor:pointer}}</style><main><svg viewBox='0 0 24 24' fill='none' stroke='currentColor' stroke-width='2' stroke-linecap='round'><path d='M12 21a9 9 0 1 1 9-9'/><path d='M12 7v5l3 2'/></svg><h1>This site can't be reached</h1><p><b>{host}</b> refused to connect.</p><p>Try:</p><ul><li>Checking that the dev server is running</li><li>Checking the address: <code>{shown}</code></li></ul><p><code>ERR_CONNECTION_REFUSED</code></p><button onclick=\"location.replace('{go}')\">Reload</button></main><script>setInterval(()=>fetch('{go}',{{mode:'no-cors',cache:'no-store'}}).then(()=>location.replace('{go}'),()=>{{}}),1500)</script></html>"
    )
}

/// The address of AgentDeck's error page for `target`.
fn error_page(target: &str) -> Result<tauri::Url, String> {
    tauri::Url::parse_with_params(&format!("{}error", base()), &[("url", target)]).map_err(|error| error.to_string())
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
        crate::bridge::ingest(&report);
        let _ = app.emit("browser-report", report);
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

fn webview(app: &AppHandle, tab: &str) -> Result<Webview, String> {
    app.get_webview(&label(tab))
        .ok_or_else(|| "the browser is not open".to_string())
}

/// Whether a local address (localhost, 127.0.0.1, [::1]) has a server on its port. Other addresses count as up:
/// only the page itself can tell. Vite may listen on IPv4 or IPv6 only, so both are tried.
pub(crate) fn answers(url: &tauri::Url) -> bool {
    if !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]" | "::1")) {
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

/// Where a tab goes for `url`: the page itself, or AgentDeck's error page when nothing listens on its local port.
fn destination(tab: &str, url: &str) -> Result<tauri::Url, String> {
    let target = parse(url)?;
    remember(tab, target.as_str());
    if answers(&target) {
        Ok(target)
    } else {
        error_page(target.as_str())
    }
}

pub(crate) fn parse(url: &str) -> Result<tauri::Url, String> {
    // A bare host or "localhost:4200" is what people type; give it a scheme.
    let url = url.trim();
    let text = if url.contains("://") || url.starts_with("about:") || url.starts_with("data:") {
        url.to_owned()
    } else if url.starts_with("localhost") || url.starts_with("127.") || url.starts_with("[::1]") {
        format!("http://{url}")
    } else if url.contains(char::is_whitespace) || !url.contains('.') {
        // Words, not an address ("zz", "svelte runes"): search for them, as a browser's address bar does.
        return tauri::Url::parse_with_params("https://www.google.com/search", &[("q", url)]).map_err(|error| error.to_string());
    } else {
        format!("https://{url}")
    };
    text.parse().map_err(|error| format!("{error}"))
}

fn place(view: &Webview, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    view.set_position(LogicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    view.set_size(LogicalSize::new(w.max(1.0), h.max(1.0)))
        .map_err(|error| error.to_string())
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
        return view.show().map_err(|error| error.to_string());
    }
    let window = app.get_window("main").ok_or("no main window")?;
    let mut builder = WebviewBuilder::new(label(&tab), WebviewUrl::External(destination(&tab, &url)?))
        .initialization_script(injected_script(&tab))
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
                    if let Some(target) = requested(&tab) {
                        if let Ok(page) = error_page(&target) {
                            let _ = view.navigate(page);
                        }
                    }
                }
                return;
            }
            if event == "started" && !ours {
                remember(&tab, &url);
                crate::bridge::page_started(&tab, &url);
            }
            let shown = if ours { requested(&tab).unwrap_or_else(|| url.clone()) } else { url };
            let _ = view.app_handle().emit(
                "browser-load",
                serde_json::json!({
                    "label": view.label(),
                    "event": event,
                    "url": shown,
                }),
            );
        });
    // A phone or tablet identifies itself as one, so sites send their mobile layout.
    if let Some(agent) = user_agent.as_deref() {
        builder = builder.user_agent(agent);
    }
    let view = window
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(w.max(1.0), h.max(1.0)),
        )
        .map_err(|error| error.to_string())?;
    allow_password_saving(&view);
    crate::cdp::watch_frames(&view);
    Ok(())
}

/// The page's own password manager: the browser offers to save a password after a sign-in and fills it next time,
/// as Edge or Chrome do. The setting belongs to the WebView2 profile, so it covers every page.
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
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    press_at(hwnd.0 as isize, px.round() as i32, py.round() as i32);
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
fn press_at(_hwnd: isize, _x: i32, _y: i32) {}

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

/// Hides every tab's page (a menu or the annotation view needs the space, or another tab comes forward).
#[tauri::command]
pub fn browser_hide(app: AppHandle) -> Result<(), String> {
    for (name, view) in app.webviews() {
        if name.starts_with(PREFIX) {
            view.hide().map_err(|error| error.to_string())?;
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
            view.set_position(LogicalPosition::new(-20000.0, -20000.0)).map_err(|error| error.to_string())?;
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
pub async fn browser_capture(app: AppHandle, tab: String) -> Result<String, String> {
    let view = webview(&app, &tab)?;
    let (sender, receiver) = tokio::sync::oneshot::channel::<Result<Vec<u8>, String>>();
    capture(&view, sender)?;
    let bytes = receiver
        .await
        .map_err(|_| "the capture was dropped".to_string())??;
    use base64::Engine;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
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
                core.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &handler)
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
