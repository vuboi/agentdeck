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
    let mut builder = WebviewBuilder::new(label(&tab), WebviewUrl::External(parse(&url)?))
        .initialization_script(injected_script(&tab))
        .on_page_load(|view, payload| {
            let event = match payload.event() {
                PageLoadEvent::Started => "started",
                PageLoadEvent::Finished => "finished",
            };
            if event == "started" {
                crate::bridge::page_started(view.label().trim_start_matches(PREFIX), payload.url().as_str());
            }
            let _ = view.app_handle().emit(
                "browser-load",
                serde_json::json!({
                    "label": view.label(),
                    "event": event,
                    "url": payload.url().to_string(),
                }),
            );
        });
    // A phone or tablet identifies itself as one, so sites send their mobile layout.
    if let Some(agent) = user_agent.as_deref() {
        builder = builder.user_agent(agent);
    }
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

#[tauri::command]
pub fn browser_navigate(app: AppHandle, tab: String, url: String) -> Result<(), String> {
    webview(&app, &tab)?
        .navigate(parse(&url)?)
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
