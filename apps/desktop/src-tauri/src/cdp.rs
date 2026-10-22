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
use tauri::Webview;

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

/// Runs a script in the page (or in a frame's session) and returns its value.
pub async fn evaluate(view: &Webview, script: &str, session: Option<String>) -> Result<Value, String> {
    let params = json!({ "expression": script, "returnByValue": true, "awaitPromise": true, "userGesture": true });
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
  let box = document.getElementById(id);
  if (!box) {
    box = document.createElement('div');
    box.id = id;
    box.style.cssText = 'position:fixed;left:0;top:0;z-index:2147483647;pointer-events:none;transition:transform 420ms cubic-bezier(.22,1,.36,1),opacity 200ms;opacity:0;will-change:transform';
    box.innerHTML = '<svg width="22" height="22" viewBox="0 0 24 24" style="filter:drop-shadow(0 2px 4px rgba(0,0,0,.45))"><path d="M4 2l15 9.2-6.6 1.5 3.8 7.4-2.9 1.4-3.8-7.4L4 19z" fill="#8b5cf6" stroke="#fff" stroke-width="1.6" stroke-linejoin="round"/></svg><span style="position:absolute;left:18px;top:18px;padding:1px 6px;border-radius:6px;background:#8b5cf6;color:#fff;font:600 10px/16px system-ui,sans-serif;white-space:nowrap">AI</span>';
    (document.body || document.documentElement).appendChild(box);
    box.style.transform = `translate(${x - 40}px, ${y - 40}px)`;
  }
  box.getBoundingClientRect();
  box.style.opacity = '1';
  box.style.transform = `translate(${x - 3}px, ${y - 2}px)`;
  if (kind === 'click') {
    setTimeout(() => {
      const ring = document.createElement('div');
      ring.style.cssText = `position:fixed;left:${x - 14}px;top:${y - 14}px;width:28px;height:28px;border-radius:50%;border:2px solid #8b5cf6;background:rgba(139,92,246,.18);z-index:2147483646;pointer-events:none`;
      (document.body || document.documentElement).appendChild(ring);
      ring.animate([{ transform: 'scale(.4)', opacity: 1 }, { transform: 'scale(1.6)', opacity: 0 }], { duration: 500, easing: 'ease-out' }).onfinish = () => ring.remove();
    }, 380);
  }
  clearTimeout(window.__agentdeckCursorTimer);
  window.__agentdeckCursorTimer = setTimeout(() => (box.style.opacity = '0'), 2500);
  return true;
})"##;

/// Shows the agent's pointer moving to a point; returns once it has arrived.
async fn show_pointer(view: &Webview, x: f64, y: f64, kind: &str) {
    let script = format!("{CURSOR}({x}, {y}, {kind:?})");
    if evaluate(view, &script, None).await.is_ok() {
        tokio::time::sleep(std::time::Duration::from_millis(420)).await;
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

/// The overlay's own picture, with a transparent background so it can be laid over the page.
pub async fn transparent_screenshot(view: &Webview) -> Result<Vec<u8>, String> {
    let clear = json!({ "color": { "r": 0, "g": 0, "b": 0, "a": 0 } });
    call(view, "Emulation.setDefaultBackgroundColorOverride", clear, None).await?;
    screenshot(view).await
}
