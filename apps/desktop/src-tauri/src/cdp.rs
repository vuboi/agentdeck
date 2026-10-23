//! The Chrome DevTools Protocol of a page, through WebView2: what a browser's devtools can do, the agent can do.
//!
//! - Input goes in as the browser's own events (trusted, with hover, into iframes of any origin).
//! - Scripts run through `Runtime.evaluate`, so a page whose policy forbids `eval` still answers.
//! - Frames of another origin run in their own process; `Target.setAutoAttach` hands each one a session, and
//!   scripts reach them through `CallDevToolsProtocolMethodForSession`.
//! - Screenshots come from the compositor (`Page.captureScreenshot`).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{Emitter, Manager, Webview};

/// A frame of another origin, attached to its own DevTools session.
#[derive(Clone, Serialize)]
pub struct Frame {
    pub session: String,
    pub target: String,
    pub url: String,
    pub kind: String,
}

/// The attached frames of every page (webview label).
static FRAMES: LazyLock<Mutex<HashMap<String, Vec<Frame>>>> = LazyLock::new(Default::default);

/// The cross-origin frames attached to a page.
pub fn frames(label: &str) -> Vec<Frame> {
    FRAMES
        .lock()
        .map(|frames| frames.get(label).cloned().unwrap_or_default())
        .unwrap_or_default()
}

/// Calls a DevTools method on a page, or on one of its frames' sessions, and waits for the answer.
pub async fn call(
    view: &Webview,
    method: &str,
    params: Value,
    session: Option<String>,
) -> Result<Value, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel::<Result<Value, String>>();
    start(view, method.to_string(), params.to_string(), session, sender)?;
    match tokio::time::timeout(Duration::from_secs(20), receiver).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err(format!("{method}: the page did not answer")),
        Err(_) => Err(format!("{method}: timed out")),
    }
}

#[cfg(windows)]
fn start(
    view: &Webview,
    method: String,
    params: String,
    session: Option<String>,
    sender: tokio::sync::oneshot::Sender<Result<Value, String>>,
) -> Result<(), String> {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_11;
    use windows::core::{Interface, HSTRING};

    view.with_webview(move |platform| {
        let reply = std::rc::Rc::new(std::cell::RefCell::new(Some(sender)));
        let answer = reply.clone();
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
            let out = result
                .map_err(|error| error.to_string())
                .and_then(|()| serde_json::from_str::<Value>(&json).map_err(|error| error.to_string()));
            if let Some(sender) = answer.borrow_mut().take() {
                let _ = sender.send(out);
            }
            Ok(())
        }));
        // SAFETY: WebView2 calls on the thread that owns the webview, as `with_webview` runs us.
        let started = unsafe {
            platform.controller().CoreWebView2().and_then(|core| match &session {
                Some(id) => core.cast::<ICoreWebView2_11>().and_then(|core| {
                    core.CallDevToolsProtocolMethodForSession(
                        &HSTRING::from(id.as_str()),
                        &HSTRING::from(method.as_str()),
                        &HSTRING::from(params.as_str()),
                        &handler,
                    )
                }),
                None => core.CallDevToolsProtocolMethod(
                    &HSTRING::from(method.as_str()),
                    &HSTRING::from(params.as_str()),
                    &handler,
                ),
            })
        };
        if let Err(error) = started {
            if let Some(sender) = reply.borrow_mut().take() {
                let _ = sender.send(Err(error.to_string()));
            }
        }
    })
    .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn start(
    _view: &Webview,
    _method: String,
    _params: String,
    _session: Option<String>,
    _sender: tokio::sync::oneshot::Sender<Result<Value, String>>,
) -> Result<(), String> {
    Err("DevTools access is only wired for WebView2 (Windows)".into())
}

/// Keeps the list of a page's cross-origin frames: each is attached to a session as it appears.
pub fn watch_frames(view: &Webview) {
    let label = view.label().to_string();
    subscribe(view, label.clone());
    let view = view.clone();
    tauri::async_runtime::spawn(async move {
        let params = json!({ "autoAttach": true, "waitForDebuggerOnStart": false, "flatten": true });
        if let Err(error) = call(&view, "Target.setAutoAttach", params, None).await {
            eprintln!("frames of {label}: {error}");
        }
    });
}

#[cfg(windows)]
fn subscribe(view: &Webview, label: String) {
    use webview2_com::DevToolsProtocolEventReceivedEventHandler;
    use windows::core::{HSTRING, PWSTR};

    let _ = view.with_webview(move |platform| {
        // SAFETY: WebView2 calls on the thread that owns the webview, as `with_webview` runs us.
        unsafe {
            let Ok(core) = platform.controller().CoreWebView2() else {
                return;
            };
            for event in ["Target.attachedToTarget", "Target.detachedFromTarget", "Target.targetInfoChanged"] {
                let Ok(receiver) = core.GetDevToolsProtocolEventReceiver(&HSTRING::from(event)) else {
                    continue;
                };
                let label = label.clone();
                let handler = DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut text = PWSTR::null();
                    args.ParameterObjectAsJson(&mut text)?;
                    let value: Value = serde_json::from_str(&webview2_com::take_pwstr(text)).unwrap_or_default();
                    remember(&label, event, &value);
                    Ok(())
                }));
                let mut token = 0i64;
                let _ = receiver.add_DevToolsProtocolEventReceived(&handler, &mut token);
            }
        }
    });
}

#[cfg(not(windows))]
fn subscribe(_view: &Webview, _label: String) {}

fn remember(label: &str, event: &str, value: &Value) {
    let Ok(mut all) = FRAMES.lock() else { return };
    let frames = all.entry(label.to_string()).or_default();
    let info = &value["targetInfo"];
    match event {
        "Target.attachedToTarget" => {
            let session = value["sessionId"].as_str().unwrap_or_default().to_string();
            frames.retain(|frame| frame.session != session);
            frames.push(Frame {
                session,
                target: info["targetId"].as_str().unwrap_or_default().to_string(),
                url: info["url"].as_str().unwrap_or_default().to_string(),
                kind: info["type"].as_str().unwrap_or_default().to_string(),
            });
        }
        "Target.detachedFromTarget" => {
            let session = value["sessionId"].as_str().unwrap_or_default();
            frames.retain(|frame| frame.session != session);
        }
        _ => {
            let target = info["targetId"].as_str().unwrap_or_default();
            if let Some(frame) = frames.iter_mut().find(|frame| frame.target == target) {
                frame.url = info["url"].as_str().unwrap_or_default().to_string();
            }
        }
    }
}

/// The network events the Network tab of the app is built from. They go to the app as `devtools-net`, untouched.
const NETWORK_EVENTS: [&str; 12] = [
    "Network.requestWillBeSent",
    "Network.responseReceived",
    "Network.loadingFinished",
    "Network.loadingFailed",
    "Network.requestServedFromCache",
    "Network.webSocketCreated",
    "Network.webSocketHandshakeResponseReceived",
    "Network.webSocketFrameSent",
    "Network.webSocketFrameReceived",
    "Network.webSocketFrameError",
    "Network.webSocketClosed",
    "Network.eventSourceMessageReceived",
];

/// Turns the page's network recording on and passes each event to the app (the Network tab of the developer tools).
pub fn watch_network(view: &Webview) {
    subscribe_network(view);
    let view = view.clone();
    tauri::async_runtime::spawn(async move {
        // Bodies are kept (up to these sizes) so the tab can show a response after the fact.
        let params = json!({ "maxTotalBufferSize": 50_000_000, "maxResourceBufferSize": 10_000_000, "maxPostDataSize": 65_536 });
        if let Err(error) = call(&view, "Network.enable", params, None).await {
            eprintln!("network of {}: {error}", view.label());
        }
    });
}

#[cfg(windows)]
fn subscribe_network(view: &Webview) {
    use webview2_com::DevToolsProtocolEventReceivedEventHandler;
    use windows::core::{HSTRING, PWSTR};

    let app = view.app_handle().clone();
    let label = view.label().to_string();
    let _ = view.with_webview(move |platform| {
        // SAFETY: WebView2 calls on the thread that owns the webview, as `with_webview` runs us.
        unsafe {
            let Ok(core) = platform.controller().CoreWebView2() else {
                return;
            };
            for event in NETWORK_EVENTS {
                let Ok(receiver) = core.GetDevToolsProtocolEventReceiver(&HSTRING::from(event)) else {
                    continue;
                };
                let (app, label) = (app.clone(), label.clone());
                let handler = DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else { return Ok(()) };
                    let mut text = PWSTR::null();
                    args.ParameterObjectAsJson(&mut text)?;
                    let params: Value = serde_json::from_str(&webview2_com::take_pwstr(text)).unwrap_or_default();
                    let _ = app.emit("devtools-net", json!({ "label": label, "event": event, "params": params }));
                    Ok(())
                }));
                let mut token = 0i64;
                let _ = receiver.add_DevToolsProtocolEventReceived(&handler, &mut token);
            }
        }
    });
}

#[cfg(not(windows))]
fn subscribe_network(_view: &Webview) {}

/// Runs a script in the page (or in a frame's session) and returns its value.
pub async fn evaluate(view: &Webview, script: &str, session: Option<String>) -> Result<Value, String> {
    // replMode: top-level await works, as in the DevTools console.
    let params = json!({ "expression": script, "returnByValue": true, "awaitPromise": true, "userGesture": true, "replMode": true });
    let answer = call(view, "Runtime.evaluate", params, session).await?;
    if let Some(details) = answer.get("exceptionDetails") {
        let text = details["exception"]["description"]
            .as_str()
            .or_else(|| details["text"].as_str())
            .unwrap_or("the script threw");
        return Err(text.to_string());
    }
    Ok(answer["result"]["value"].clone())
}

/// The agent's pointer, drawn in the page so the user sees where it acts: a cursor glides to the point, then a
/// ring pulses for a click. It never takes events and fades after a moment of rest.
const CURSOR: &str = r##"((x, y, kind) => {
  const id = '__agentdeck_cursor';
  const root = document.body || document.documentElement;
  let box = document.getElementById(id);
  if (!box) {
    box = document.createElement('div');
    box.id = id;
    box.style.cssText = 'position:fixed;left:0;top:0;z-index:2147483647;pointer-events:none;opacity:0;will-change:transform;transition:transform 520ms cubic-bezier(.2,.9,.25,1),opacity 220ms ease';
    box.innerHTML = `
      <div data-halo style="position:absolute;left:-14px;top:-14px;width:40px;height:40px;border-radius:50%;background:radial-gradient(circle,rgba(124,92,255,.35),rgba(124,92,255,0) 70%);filter:blur(2px)"></div>
      <svg data-arrow width="20" height="20" viewBox="0 0 20 20" style="position:relative;display:block;transform-origin:2px 2px;transition:transform 140ms ease;filter:drop-shadow(0 1px 1.5px rgba(0,0,0,.35)) drop-shadow(0 4px 10px rgba(0,0,0,.18))">
        <path d="M2.2 1.6c-.3-.2-.7.1-.6.5l3.9 15.1c.1.4.7.5.9.1l2.5-4.9c.1-.2.3-.3.5-.3l5.4-.4c.4 0 .6-.5.3-.8L2.2 1.6z" fill="#111827" stroke="#ffffff" stroke-width="1.4" stroke-linejoin="round"/>
      </svg>
      <div style="position:absolute;left:16px;top:17px;display:flex;align-items:center;gap:4px;padding:2px 8px 2px 6px;border-radius:999px;background:linear-gradient(135deg,#7c5cff,#5b8cff);box-shadow:0 2px 8px rgba(91,92,255,.35),inset 0 0 0 1px rgba(255,255,255,.25);color:#fff;font:600 10.5px/16px ui-sans-serif,system-ui,-apple-system,'Segoe UI',sans-serif;letter-spacing:.01em;white-space:nowrap">
        <svg width="10" height="10" viewBox="0 0 24 24" fill="#fff"><path d="M12 2l1.9 6.1L20 10l-6.1 1.9L12 18l-1.9-6.1L4 10l6.1-1.9z"/></svg>AI
      </div>`;
    root.appendChild(box);
    box.style.transform = `translate(${x + 24}px, ${y + 24}px)`;
  }
  box.getBoundingClientRect();
  box.style.opacity = '1';
  box.style.transform = `translate(${x - 2}px, ${y - 2}px)`;
  if (kind === 'click') {
    setTimeout(() => {
      const arrow = box.querySelector('[data-arrow]');
      if (arrow) {
        arrow.style.transform = 'scale(.82)';
        setTimeout(() => (arrow.style.transform = ''), 140);
      }
      for (const [delay, size] of [[0, 34], [90, 52]]) {
        const ring = document.createElement('div');
        ring.style.cssText = `position:fixed;left:${x - size / 2}px;top:${y - size / 2}px;width:${size}px;height:${size}px;border-radius:50%;border:1.5px solid rgba(124,92,255,.85);background:rgba(124,92,255,.10);z-index:2147483646;pointer-events:none`;
        root.appendChild(ring);
        ring.animate([{ transform: 'scale(.3)', opacity: 0.9 }, { transform: 'scale(1)', opacity: 0 }], { duration: 520, delay, easing: 'cubic-bezier(.2,.8,.2,1)', fill: 'both' }).onfinish = () => ring.remove();
      }
    }, 470);
  }
  clearTimeout(window.__agentdeckCursorTimer);
  window.__agentdeckCursorTimer = setTimeout(() => (box.style.opacity = '0'), 3000);
  return true;
})"##;

/// Shows the agent's pointer moving to a point; returns once it has arrived.
async fn show_pointer(view: &Webview, x: f64, y: f64, kind: &str) {
    let script = format!("{CURSOR}({x}, {y}, {kind:?})");
    if evaluate(view, &script, None).await.is_ok() {
        tokio::time::sleep(std::time::Duration::from_millis(520)).await;
    }
}

/// A press at a point of the page (CSS pixels of the viewport), as the browser's own mouse events.
pub async fn click(view: &Webview, x: f64, y: f64) -> Result<(), String> {
    show_pointer(view, x, y, "click").await;
    let at = |kind: &str, buttons: u8| {
        json!({ "type": kind, "x": x, "y": y, "button": "left", "buttons": buttons, "clickCount": 1 })
    };
    call(view, "Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": x, "y": y }), None).await?;
    call(view, "Input.dispatchMouseEvent", at("mousePressed", 1), None).await?;
    call(view, "Input.dispatchMouseEvent", at("mouseReleased", 0), None).await?;
    Ok(())
}

/// Moves the mouse over a point (hover menus, tooltips).
pub async fn hover(view: &Webview, x: f64, y: f64) -> Result<(), String> {
    show_pointer(view, x, y, "hover").await;
    call(view, "Input.dispatchMouseEvent", json!({ "type": "mouseMoved", "x": x, "y": y }), None).await?;
    Ok(())
}

/// Scrolls at a point, as a mouse wheel would.
pub async fn scroll(view: &Webview, x: f64, y: f64, dx: f64, dy: f64) -> Result<(), String> {
    show_pointer(view, x, y, "scroll").await;
    let params = json!({ "type": "mouseWheel", "x": x, "y": y, "deltaX": dx, "deltaY": dy });
    call(view, "Input.dispatchMouseEvent", params, None).await?;
    Ok(())
}

/// Types text into whatever has focus, as a keyboard would.
pub async fn type_text(view: &Webview, text: &str) -> Result<(), String> {
    call(view, "Input.insertText", json!({ "text": text }), None).await?;
    Ok(())
}

/// Presses a named key (Enter, Tab, Escape, Backspace, Delete, ArrowUp/Down/Left/Right, Home, End, PageUp/Down).
pub async fn key(view: &Webview, name: &str) -> Result<(), String> {
    let code: i64 = match name {
        "Enter" => 13,
        "Tab" => 9,
        "Escape" => 27,
        "Backspace" => 8,
        "Delete" => 46,
        "ArrowLeft" => 37,
        "ArrowUp" => 38,
        "ArrowRight" => 39,
        "ArrowDown" => 40,
        "Home" => 36,
        "End" => 35,
        "PageUp" => 33,
        "PageDown" => 34,
        " " | "Space" => 32,
        _ => return Err(format!("unknown key {name}; use Enter, Tab, Escape, Backspace, Delete, Arrow*, Home, End, PageUp, PageDown, Space")),
    };
    let key = if name == "Space" { " " } else { name };
    let text = match name {
        "Enter" => "\r",
        " " | "Space" => " ",
        _ => "",
    };
    let mut down = json!({ "type": "keyDown", "key": key, "code": key, "windowsVirtualKeyCode": code });
    if !text.is_empty() {
        down["text"] = json!(text);
    }
    call(view, "Input.dispatchKeyEvent", down, None).await?;
    let up = json!({ "type": "keyUp", "key": key, "code": key, "windowsVirtualKeyCode": code });
    call(view, "Input.dispatchKeyEvent", up, None).await?;
    Ok(())
}

/// The page as the compositor draws it, PNG bytes.
pub async fn screenshot(view: &Webview) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let answer = call(view, "Page.captureScreenshot", json!({ "format": "png" }), None).await?;
    let data = answer["data"].as_str().ok_or("no screenshot data")?;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| error.to_string())
}

