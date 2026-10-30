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
    call_for(view, method, params, session, 20).await
}

/// `call`, waiting up to `seconds` for the answer (a script that polls the page for a while).
pub async fn call_for(
    view: &Webview,
    method: &str,
    params: Value,
    session: Option<String>,
    seconds: u64,
) -> Result<Value, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel::<Result<Value, String>>();
    start(
        view,
        method.to_string(),
        params.to_string(),
        session,
        sender,
    )?;
    match tokio::time::timeout(Duration::from_secs(seconds), receiver).await {
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
        let handler =
            CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |result, json| {
                let out = result.map_err(|error| error.to_string()).and_then(|()| {
                    serde_json::from_str::<Value>(&json).map_err(|error| error.to_string())
                });
                if let Some(sender) = answer.borrow_mut().take() {
                    let _ = sender.send(out);
                }
                Ok(())
            }));
        // SAFETY: WebView2 calls on the thread that owns the webview, as `with_webview` runs us.
        let started = unsafe {
            platform
                .controller()
                .CoreWebView2()
                .and_then(|core| match &session {
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

/// Empties the browser's HTTP and Cache Storage caches. WebView2 refuses `Network.clearBrowserCache`
/// (0x80070057), so this goes through the profile instead.
#[cfg(windows)]
pub async fn clear_cache(view: &Webview) -> Result<(), String> {
    use webview2_com::ClearBrowsingDataCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Profile2, ICoreWebView2_13, COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE,
        COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE,
    };
    use windows::core::Interface;

    let (sender, receiver) = tokio::sync::oneshot::channel::<Result<(), String>>();
    view.with_webview(move |platform| {
        let reply = std::rc::Rc::new(std::cell::RefCell::new(Some(sender)));
        let answer = reply.clone();
        let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |result| {
            if let Some(sender) = answer.borrow_mut().take() {
                let _ = sender.send(result.map_err(|error| error.to_string()));
            }
            Ok(())
        }));
        // SAFETY: WebView2 calls on the thread that owns the webview, as `with_webview` runs us.
        let started = unsafe {
            platform
                .controller()
                .CoreWebView2()
                .and_then(|core| core.cast::<ICoreWebView2_13>())
                .and_then(|core| core.Profile())
                .and_then(|profile| profile.cast::<ICoreWebView2Profile2>())
                .and_then(|profile| {
                    profile.ClearBrowsingData(
                        COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE
                            | COREWEBVIEW2_BROWSING_DATA_KINDS_CACHE_STORAGE,
                        &handler,
                    )
                })
        };
        if let Err(error) = started {
            if let Some(sender) = reply.borrow_mut().take() {
                let _ = sender.send(Err(error.to_string()));
            }
        }
    })
    .map_err(|error| error.to_string())?;
    match tokio::time::timeout(Duration::from_secs(20), receiver).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("clearing the cache: the page did not answer".into()),
        Err(_) => Err("clearing the cache: timed out".into()),
    }
}

#[cfg(not(windows))]
pub async fn clear_cache(_view: &Webview) -> Result<(), String> {
    Err("Clearing the cache is only wired for WebView2 (Windows)".into())
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
        let params =
            json!({ "autoAttach": true, "waitForDebuggerOnStart": false, "flatten": true });
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
            for event in [
                "Target.attachedToTarget",
                "Target.detachedFromTarget",
                "Target.targetInfoChanged",
            ] {
                let Ok(receiver) = core.GetDevToolsProtocolEventReceiver(&HSTRING::from(event))
                else {
                    continue;
                };
                let label = label.clone();
                let handler =
                    DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else { return Ok(()) };
                        let mut text = PWSTR::null();
                        args.ParameterObjectAsJson(&mut text)?;
                        let value: Value = serde_json::from_str(&webview2_com::take_pwstr(text))
                            .unwrap_or_default();
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
                let Ok(receiver) = core.GetDevToolsProtocolEventReceiver(&HSTRING::from(event))
                else {
                    continue;
                };
                let (app, label) = (app.clone(), label.clone());
                let handler =
                    DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else { return Ok(()) };
                        let mut text = PWSTR::null();
                        args.ParameterObjectAsJson(&mut text)?;
                        let params: Value = serde_json::from_str(&webview2_com::take_pwstr(text))
                            .unwrap_or_default();
                        let _ = app.emit(
                            "devtools-net",
                            json!({ "label": label, "event": event, "params": params }),
                        );
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
pub async fn evaluate(
    view: &Webview,
    script: &str,
    session: Option<String>,
) -> Result<Value, String> {
    evaluate_for(view, script, session, 20).await
}

/// `evaluate`, letting the script run up to `seconds` (statements, top-level await, the last expression is the value).
pub async fn evaluate_for(
    view: &Webview,
    script: &str,
    session: Option<String>,
    seconds: u64,
) -> Result<Value, String> {
    // replMode: top-level await works, as in the DevTools console.
    let params = json!({ "expression": script, "returnByValue": true, "awaitPromise": true, "userGesture": true, "replMode": true });
    let answer = call_for(view, "Runtime.evaluate", params, session, seconds).await?;
    if let Some(details) = answer.get("exceptionDetails") {
        let text = details["exception"]["description"]
            .as_str()
            .or_else(|| details["text"].as_str())
            .unwrap_or("the script threw");
        return Err(text.to_string());
    }
    Ok(answer["result"]["value"].clone())
}

/// Makes a page work for the agent while it is not the one in front or the app is in the background: the page
/// believes it has focus (`:focus`, `focus`/`blur`, `document.hasFocus()`), and it is kept in the active
/// lifecycle state so timers and animation frames keep running. Best effort: a page that refuses one still works.
pub async fn prepare(view: &Webview) {
    let _ = call(
        view,
        "Emulation.setFocusEmulationEnabled",
        json!({ "enabled": true }),
        None,
    )
    .await;
    let _ = call(
        view,
        "Page.setWebLifecycleState",
        json!({ "state": "active" }),
        None,
    )
    .await;
}

/// The agent's pointer, drawn in the page so the user sees where it acts: a cursor glides to the point, then a
/// ring pulses for a click. It lives in a Shadow DOM, takes no events (`pointer-events: none`, above everything)
/// and fades after a moment of rest. Its colour and label tell the agents of several chats apart.
const CURSOR: &str = r##"((x, y, kind, label) => {
  const id = '__agentdeck_cursor';
  const root = document.documentElement;
  let hue = 0;
  for (const ch of label) hue = (hue * 31 + ch.charCodeAt(0)) % 360;
  const tint = `hsl(${hue} 85% 62%)`;
  const tintSoft = `hsl(${hue} 85% 62% / .35)`;
  const tintDark = `hsl(${(hue + 30) % 360} 80% 52%)`;
  let host = document.getElementById(id);
  let shadow = host && host.shadowRoot;
  if (!host || !shadow) {
    host = document.createElement('div');
    host.id = id;
    host.style.cssText = 'all:initial;position:fixed;left:0;top:0;width:0;height:0;z-index:2147483647;pointer-events:none';
    shadow = host.attachShadow({ mode: 'open' });
    shadow.innerHTML = `
      <style>
        *, :host { pointer-events: none !important; box-sizing: border-box; }
        .box { position: fixed; left: 0; top: 0; opacity: 0; will-change: transform; transition: transform 520ms cubic-bezier(.2,.9,.25,1), opacity 220ms ease; }
        .halo { position: absolute; left: -14px; top: -14px; width: 40px; height: 40px; border-radius: 50%; filter: blur(2px); }
        .arrow { position: relative; display: block; transform-origin: 2px 2px; transition: transform 140ms ease; filter: drop-shadow(0 1px 1.5px rgba(0,0,0,.35)) drop-shadow(0 4px 10px rgba(0,0,0,.18)); }
        .tag { position: absolute; left: 16px; top: 17px; display: flex; align-items: center; gap: 4px; padding: 2px 8px 2px 6px; border-radius: 999px; color: #fff; font: 600 10.5px/16px ui-sans-serif, system-ui, -apple-system, 'Segoe UI', sans-serif; letter-spacing: .01em; white-space: nowrap; box-shadow: 0 2px 8px rgba(0,0,0,.25), inset 0 0 0 1px rgba(255,255,255,.25); }
        .ring { position: fixed; border-radius: 50%; }
      </style>
      <div class="box">
        <div class="halo"></div>
        <svg class="arrow" width="20" height="20" viewBox="0 0 20 20">
          <path d="M2.2 1.6c-.3-.2-.7.1-.6.5l3.9 15.1c.1.4.7.5.9.1l2.5-4.9c.1-.2.3-.3.5-.3l5.4-.4c.4 0 .6-.5.3-.8L2.2 1.6z" fill="#111827" stroke="#ffffff" stroke-width="1.4" stroke-linejoin="round"/>
        </svg>
        <div class="tag"><svg width="10" height="10" viewBox="0 0 24 24" fill="#fff"><path d="M12 2l1.9 6.1L20 10l-6.1 1.9L12 18l-1.9-6.1L4 10l6.1-1.9z"/></svg><span data-label></span></div>
      </div>`;
    root.appendChild(host);
    const first = shadow.querySelector('.box');
    first.style.transform = `translate(${x + 24}px, ${y + 24}px)`;
  }
  const box = shadow.querySelector('.box');
  box.querySelector('.halo').style.background = `radial-gradient(circle, ${tintSoft}, transparent 70%)`;
  const tag = box.querySelector('.tag');
  tag.style.background = `linear-gradient(135deg, ${tint}, ${tintDark})`;
  tag.querySelector('[data-label]').textContent = label;
  box.getBoundingClientRect();
  box.style.opacity = '1';
  box.style.transform = `translate(${x - 2}px, ${y - 2}px)`;
  if (kind === 'click') {
    setTimeout(() => {
      const arrow = box.querySelector('.arrow');
      arrow.style.transform = 'scale(.82)';
      setTimeout(() => (arrow.style.transform = ''), 140);
      for (const [delay, size] of [[0, 34], [90, 52]]) {
        const ring = document.createElement('div');
        ring.className = 'ring';
        ring.style.cssText = `left:${x - size / 2}px;top:${y - size / 2}px;width:${size}px;height:${size}px;border:1.5px solid ${tint};background:${tintSoft}`;
        shadow.appendChild(ring);
        ring.animate([{ transform: 'scale(.3)', opacity: 0.9 }, { transform: 'scale(1)', opacity: 0 }], { duration: 520, delay, easing: 'cubic-bezier(.2,.8,.2,1)', fill: 'both' }).onfinish = () => ring.remove();
      }
    }, 470);
  }
  clearTimeout(window.__agentdeckCursorTimer);
  window.__agentdeckCursorTimer = setTimeout(() => (box.style.opacity = '0'), 3000);
  return true;
})"##;

/// Shows the agent's pointer, labelled `who`, moving to a point; returns once it has arrived.
async fn show_pointer(view: &Webview, x: f64, y: f64, kind: &str, who: &str) {
    let label = if who.is_empty() { "AI" } else { who };
    let script = format!("{CURSOR}({x}, {y}, {kind:?}, {})", json!(label));
    // Without DevTools access the page runs it as an ordinary script; nothing to wait for but the glide.
    if evaluate(view, &script, None).await.is_ok() || view.eval(script.as_str()).is_ok() {
        tokio::time::sleep(std::time::Duration::from_millis(520)).await;
    }
}

/// A press at a point of the page (CSS pixels of the viewport), as the browser's own mouse events.
pub async fn click(view: &Webview, x: f64, y: f64, who: &str) -> Result<(), String> {
    show_pointer(view, x, y, "click", who).await;
    let at = |kind: &str, buttons: u8| json!({ "type": kind, "x": x, "y": y, "button": "left", "buttons": buttons, "clickCount": 1 });
    call(
        view,
        "Input.dispatchMouseEvent",
        json!({ "type": "mouseMoved", "x": x, "y": y }),
        None,
    )
    .await?;
    call(
        view,
        "Input.dispatchMouseEvent",
        at("mousePressed", 1),
        None,
    )
    .await?;
    call(
        view,
        "Input.dispatchMouseEvent",
        at("mouseReleased", 0),
        None,
    )
    .await?;
    Ok(())
}

/// Moves the mouse over a point (hover menus, tooltips).
pub async fn hover(view: &Webview, x: f64, y: f64, who: &str) -> Result<(), String> {
    show_pointer(view, x, y, "hover", who).await;
    call(
        view,
        "Input.dispatchMouseEvent",
        json!({ "type": "mouseMoved", "x": x, "y": y }),
        None,
    )
    .await?;
    Ok(())
}

/// Scrolls at a point, as a mouse wheel would.
pub async fn scroll(
    view: &Webview,
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    who: &str,
) -> Result<(), String> {
    show_pointer(view, x, y, "scroll", who).await;
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
    let mut down =
        json!({ "type": "keyDown", "key": key, "code": key, "windowsVirtualKeyCode": code });
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
    let answer = call(
        view,
        "Page.captureScreenshot",
        json!({ "format": "png" }),
        None,
    )
    .await?;
    let data = answer["data"].as_str().ok_or("no screenshot data")?;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| error.to_string())
}

/// Captures the visible viewport at a higher output scale without resizing the live page.
pub async fn screenshot_scaled(view: &Webview, scale: f64) -> Result<Vec<u8>, String> {
    use base64::Engine;

    let scale = scale.clamp(1.0, 3.0);
    let metrics = call(view, "Page.getLayoutMetrics", json!({}), None).await?;
    let viewport = metrics
        .get("cssVisualViewport")
        .or_else(|| metrics.get("cssLayoutViewport"))
        .ok_or("page viewport metrics are unavailable")?;
    let width = viewport["clientWidth"]
        .as_f64()
        .ok_or("page viewport width is unavailable")?;
    let height = viewport["clientHeight"]
        .as_f64()
        .ok_or("page viewport height is unavailable")?;
    if width <= 0.0 || height <= 0.0 {
        return Err("page viewport is empty".into());
    }
    let answer = call(
        view,
        "Page.captureScreenshot",
        json!({
            "format": "png",
            "clip": { "x": 0, "y": 0, "width": width, "height": height, "scale": scale },
            "captureBeyondViewport": false
        }),
        None,
    )
    .await?;
    let data = answer["data"]
        .as_str()
        .ok_or("the browser returned no screenshot data")?;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| error.to_string())
}

/// Captures the complete scrollable document without moving the live page.
pub async fn screenshot_full_page(view: &Webview) -> Result<Vec<u8>, String> {
    use base64::Engine;

    let metrics = call(view, "Page.getLayoutMetrics", json!({}), None).await?;
    let content = metrics
        .get("cssContentSize")
        .or_else(|| metrics.get("contentSize"))
        .ok_or("page content size is unavailable")?;
    let width = content["width"]
        .as_f64()
        .ok_or("page content width is unavailable")?
        .ceil();
    let height = content["height"]
        .as_f64()
        .ok_or("page content height is unavailable")?
        .ceil();
    if width <= 0.0 || height <= 0.0 {
        return Err("page content is empty".into());
    }
    if width > 16_000.0 || height > 16_000.0 {
        return Err("the page is too large for one full-page screenshot".into());
    }

    let answer = call(
        view,
        "Page.captureScreenshot",
        json!({
            "format": "png",
            "clip": { "x": 0, "y": 0, "width": width, "height": height, "scale": 1 },
            "captureBeyondViewport": true
        }),
        None,
    )
    .await?;
    let data = answer["data"]
        .as_str()
        .ok_or("the browser returned no full-page screenshot data")?;
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|error| error.to_string())
}

/// Only the given part of the page (CSS pixels), PNG bytes: a picture of one element.
pub async fn screenshot_clip(
    view: &Webview,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<Vec<u8>, String> {
    use std::io::Cursor;

    // CDP's `clip` path can resize the compositor surface during capture, which briefly flashes the live page.
    // Capture the unchanged viewport instead, then crop the returned PNG offscreen.
    let metrics = call(view, "Page.getLayoutMetrics", json!({}), None).await?;
    let viewport = metrics
        .get("cssVisualViewport")
        .or_else(|| metrics.get("cssLayoutViewport"))
        .ok_or("page viewport metrics are unavailable")?;
    let viewport_w = viewport["clientWidth"]
        .as_f64()
        .ok_or("page viewport width is unavailable")?;
    let viewport_h = viewport["clientHeight"]
        .as_f64()
        .ok_or("page viewport height is unavailable")?;
    if viewport_w <= 0.0 || viewport_h <= 0.0 {
        return Err("page viewport is empty".into());
    }

    let bytes = screenshot(view).await?;
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|error| error.to_string())?;
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err("screenshot has an unsupported indexed color format".into())
        }
    };

    let scale_x = f64::from(info.width) / viewport_w;
    let scale_y = f64::from(info.height) / viewport_h;
    let left = (x.max(0.0) * scale_x).floor().max(0.0) as u32;
    let top = (y.max(0.0) * scale_y).floor().max(0.0) as u32;
    let right = ((x + w).min(viewport_w) * scale_x)
        .ceil()
        .min(f64::from(info.width)) as u32;
    let bottom = ((y + h).min(viewport_h) * scale_y)
        .ceil()
        .min(f64::from(info.height)) as u32;
    if right <= left || bottom <= top {
        return Err("selected element is outside the visible page".into());
    }

    let bytes_per_pixel = channels;
    let row_bytes = info.line_size;
    let mut cropped =
        Vec::with_capacity((right - left) as usize * (bottom - top) as usize * bytes_per_pixel);
    for row in top..bottom {
        let start = row as usize * row_bytes + left as usize * bytes_per_pixel;
        let end = row as usize * row_bytes + right as usize * bytes_per_pixel;
        cropped.extend_from_slice(&pixels[start..end]);
    }

    let mut output = Vec::new();
    let mut encoder = png::Encoder::new(&mut output, right - left, bottom - top);
    encoder.set_color(info.color_type);
    encoder.set_depth(info.bit_depth);
    encoder
        .write_header()
        .map_err(|error| error.to_string())?
        .write_image_data(&cropped)
        .map_err(|error| error.to_string())?;
    Ok(output)
}
