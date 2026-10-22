//! The AgentDeck browser: a child webview laid over the preview panel.
//!
//! The page runs in its own webview, so it behaves like a normal browser tab. A script injected into every
//! page (browser/inject.js) reports console, network and navigation, and answers requests for the DOM, a
//! screenshot and the element picker. Reports come back through the `agentdeck` URL scheme and are forwarded
//! to the app window as `browser-report` events.

use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewBuilder, WebviewUrl,
};

const LABEL: &str = "browser";

/// Where the page posts its reports: the custom scheme, which WebView2 serves under a localhost name.
fn base() -> &'static str {
    if cfg!(windows) {
        "http://agentdeck.localhost/"
    } else {
        "agentdeck://localhost/"
    }
}

fn injected_script() -> String {
    format!(
        "{}\n{}",
        include_str!("../browser/html-to-image.js"),
        include_str!("../browser/inject.js").replace("__BASE__", base())
    )
}

/// The handler of the `agentdeck` scheme: forwards a page's JSON report to the app window.
pub fn handle_report(app: &AppHandle, body: &[u8]) {
    if let Ok(report) = serde_json::from_slice::<serde_json::Value>(body) {
        let _ = app.emit("browser-report", report);
    }
}

fn webview(app: &AppHandle) -> Result<Webview, String> {
    app.get_webview(LABEL)
        .ok_or_else(|| "the browser is not open".to_string())
}

fn parse(url: &str) -> Result<tauri::Url, String> {
    // A bare host or "localhost:4200" is what people type; give it a scheme.
    let text = if url.contains("://") || url.starts_with("about:") || url.starts_with("data:") {
        url.to_owned()
    } else if url.starts_with("localhost") || url.starts_with("127.") || url.starts_with("[::1]") {
        format!("http://{url}")
    } else {
        format!("https://{url}")
    };
    text.parse().map_err(|error| format!("{error}"))
}

fn place(view: &Webview, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    if cfg!(debug_assertions) && std::env::var_os("AGENTDECK_OPEN_PREVIEW").is_some() {
        eprintln!("place: x={x:.0} y={y:.0} w={w:.0} h={h:.0}");
    }
    view.set_position(LogicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    view.set_size(LogicalSize::new(w.max(1.0), h.max(1.0)))
        .map_err(|error| error.to_string())
}

/// Opens the browser at `url` inside the given rectangle (window coordinates), or moves it there if it is open.
/// (async on purpose: creating a webview from a synchronous command deadlocks on Windows.)
#[tauri::command]
pub async fn browser_show(
    app: AppHandle,
    url: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<(), String> {
    if let Ok(view) = webview(&app) {
        place(&view, x, y, w, h)?;
        return view.show().map_err(|error| error.to_string());
    }
    let window = app.get_window("main").ok_or("no main window")?;
    let builder = WebviewBuilder::new(LABEL, WebviewUrl::External(parse(&url)?))
        .initialization_script(injected_script());
    window
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(w.max(1.0), h.max(1.0)),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Follows the panel when it moves or resizes.
#[tauri::command]
pub fn browser_bounds(app: AppHandle, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    place(&webview(&app)?, x, y, w, h)
}

/// Hides the page (a menu or the annotation view needs the space) without closing it.
#[tauri::command]
pub fn browser_hide(app: AppHandle) -> Result<(), String> {
    match webview(&app) {
        Ok(view) => view.hide().map_err(|error| error.to_string()),
        Err(_) => Ok(()),
    }
}

#[tauri::command]
pub fn browser_navigate(app: AppHandle, url: String) -> Result<(), String> {
    webview(&app)?
        .navigate(parse(&url)?)
        .map_err(|error| error.to_string())
}

/// Runs a snippet in the page: back, forward, reload, or a request to the injected script.
#[tauri::command]
pub fn browser_eval(app: AppHandle, script: String) -> Result<(), String> {
    webview(&app)?
        .eval(script)
        .map_err(|error| error.to_string())
}

/// Page zoom, used to fit a phone or tablet width into the panel.
#[tauri::command]
pub fn browser_zoom(app: AppHandle, factor: f64) -> Result<(), String> {
    webview(&app)?
        .set_zoom(factor.clamp(0.25, 3.0))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn browser_close(app: AppHandle) -> Result<(), String> {
    match webview(&app) {
        Ok(view) => view.close().map_err(|error| error.to_string()),
        Err(_) => Ok(()),
    }
}

/// Developer check (set `AGENTDECK_BROWSER_SELFTEST=<url>`): opens the browser on a page, asks it for its DOM and
/// a screenshot and prints what the injected script reports, then quits. It proves the whole round trip.
pub fn selftest(app: AppHandle, url: String) {
    use std::time::Duration;
    use tauri::Listener;
    app.listen("browser-report", |event| {
        let payload = event.payload();
        let shown: String = payload.chars().take(220).collect();
        eprintln!("report ({} bytes): {shown}", payload.len());
    });
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        eprintln!(
            "show: {:?}",
            browser_show(app.clone(), url, 280.0, 60.0, 600.0, 500.0).await
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
        eprintln!(
            "eval dom: {:?}",
            browser_eval(app.clone(), "window.__agentdeck.run('t1','dom')".into())
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
        eprintln!(
            "eval shot: {:?}",
            browser_eval(app.clone(), "window.__agentdeck.run('t2','shot')".into())
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
        eprintln!(
            "eval console: {:?}",
            browser_eval(
                app.clone(),
                "console.error('selftest error'); fetch('/favicon.ico')".into()
            )
        );
        tokio::time::sleep(Duration::from_secs(8)).await;
        app.exit(0);
    });
}
