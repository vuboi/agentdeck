//! A small HTTP server on 127.0.0.1 that lets the agent read what the AgentDeck browser shows: the console, the
//! network, the DOM, and run a script in the page. The address is put in `AGENTDECK_BRIDGE` (the agent process
//! inherits it) and named in the agent system prompt. It only listens on the loopback interface.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;

const LIMIT: usize = 500;

#[derive(Default)]
struct Tab {
    console: VecDeque<Value>,
    network: VecDeque<Value>,
    url: String,
    title: String,
}

#[derive(Default)]
struct State {
    active: String,
    tabs: HashMap<String, Tab>,
    waiting: HashMap<String, oneshot::Sender<Value>>,
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(Default::default);
static NEXT: AtomicU64 = AtomicU64::new(1);

fn push(list: &mut VecDeque<Value>, value: Value) {
    list.push_back(value);
    while list.len() > LIMIT {
        list.pop_front();
    }
}

/// The tab the user is looking at: what the agent's requests are about.
pub fn set_active(tab: &str) {
    if let Ok(mut state) = STATE.lock() {
        if state.active != tab {
            state.active = tab.to_string();
        }
    }
}

/// A page started loading: what it logged before is gone, as in devtools.
pub fn page_started(tab: &str, url: &str) {
    if let Ok(mut state) = STATE.lock() {
        let entry = state.tabs.entry(tab.to_string()).or_default();
        entry.console.clear();
        entry.network.clear();
        entry.url = url.to_string();
    }
}

/// Every report of a page passes here: it is kept for the agent, and an answer to a request wakes its waiter.
pub fn ingest(report: &Value) {
    let tab = report["tab"].as_str().unwrap_or_default().to_string();
    let kind = report["kind"].as_str().unwrap_or_default();
    let Ok(mut state) = STATE.lock() else { return };
    match kind {
        "console" => push(&mut state.tabs.entry(tab).or_default().console, report.clone()),
        "network" => push(&mut state.tabs.entry(tab).or_default().network, report.clone()),
        "nav" => {
            let entry = state.tabs.entry(tab).or_default();
            entry.url = report["url"].as_str().unwrap_or_default().to_string();
            entry.title = report["title"].as_str().unwrap_or_default().to_string();
        }
        _ => {}
    }
    if let Some(id) = report["id"].as_str() {
        if let Some(sender) = state.waiting.remove(id) {
            let _ = sender.send(report.clone());
        }
    }
}

/// Starts the server; its address goes to `AGENTDECK_BRIDGE`.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Ok(listener) = TcpListener::bind("127.0.0.1:0").await else {
            return;
        };
        let Ok(address) = listener.local_addr() else {
            return;
        };
        std::env::set_var("AGENTDECK_BRIDGE", format!("http://{address}"));
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = serve(stream, app).await;
            });
        }
    });
}

async fn serve(mut stream: TcpStream, app: AppHandle) -> std::io::Result<()> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    let (head_end, length) = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(());
        }
        data.extend_from_slice(&chunk[..read]);
        if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&data[..end]).to_lowercase();
            let length = head
                .lines()
                .find_map(|line| {
                    line.strip_prefix("content-length:")
                        .and_then(|v| v.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            break (end + 4, length.min(1 << 20));
        }
        if data.len() > 1 << 16 {
            return Ok(());
        }
    };
    while data.len() < head_end + length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        data.extend_from_slice(&chunk[..read]);
    }
    let head = String::from_utf8_lossy(&data[..head_end]).to_string();
    let body = String::from_utf8_lossy(&data[head_end..]).to_string();
    let target = head
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    let (path, query) = target.split_once('?').unwrap_or((&target, ""));
    let params: HashMap<String, String> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(key), decode(value))
        })
        .collect();
    // The same browser as an MCP server, for agents that take tools rather than curl.
    if path == "/mcp" {
        let reply = if head.starts_with("POST") {
            match crate::mcp::handle(&app, &body).await {
                Some(text) => format!(
                    "HTTP/1.1 200 OK
Content-Type: application/json
Content-Length: {}
Connection: close

{text}",
                    text.len()
                ),
                None => "HTTP/1.1 202 Accepted
Content-Length: 0
Connection: close

".to_string(),
            }
        } else {
            "HTTP/1.1 405 Method Not Allowed
Allow: POST
Content-Length: 0
Connection: close

".to_string()
        };
        stream.write_all(reply.as_bytes()).await?;
        return stream.shutdown().await;
    }
    let (status, text) = match answer(&app, path, &params, &body).await {
        Ok(value) => (
            200,
            serde_json::to_string_pretty(&value).unwrap_or_default(),
        ),
        Err(error) => (400, json!({ "error": error }).to_string()),
    };
    let reply = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
    stream.write_all(reply.as_bytes()).await?;
    stream.shutdown().await
}

fn decode(text: &str) -> String {
    let bytes = text.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&String::from_utf8_lossy(&bytes[i + 1..i + 3]), 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn level_group(level: &str) -> &str {
    match level {
        "error" => "error",
        "warn" => "warn",
        _ => "info",
    }
}

fn tail(lines: Vec<Value>, limit: usize) -> Value {
    let skip = lines.len().saturating_sub(limit);
    Value::Array(lines.into_iter().skip(skip).collect())
}

pub(crate) async fn answer(
    app: &AppHandle,
    path: &str,
    params: &HashMap<String, String>,
    body: &str,
) -> Result<Value, String> {
    let active = STATE.lock().map_err(|_| "busy".to_string())?.active.clone();
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(100);
    match path {
        "/" | "/help" => Ok(json!({
            "GET /page": "address and title of the tab the user is looking at",
            "GET /console?level=error|warn|info&limit=100": "console lines and uncaught JavaScript errors",
            "GET /network?failed=1&limit=100": "requests; failed ones (status 0 or >= 400) carry the start of the response body",
            "GET /dom?selector=css&frame=": "outerHTML of the page, or of the elements matching a selector (frame: see /frames)",
            "POST /eval?frame= (body: JavaScript)": "runs the script in the page, or in a frame of any origin, and returns its value",
            "GET /frames": "the frames of other origins in the page (iframes), each usable as ?frame=<index or part of its url>",
            "GET /shot": "a PNG of the page as shown now, with any menu open over it; answers with the file path",
            "GET /click?x=&y=": "a click at a point of the page (CSS pixels, as in /shot), as the browser's own mouse events; reaches iframes",
            "GET /hover?x=&y=": "moves the mouse over a point (hover menus, tooltips)",
            "GET /scroll?x=&y=&dy=&dx=": "scrolls at a point, as a mouse wheel (dy > 0 scrolls down)",
            "GET /type?text=": "types text into the focused field, as a keyboard",
            "GET /key?key=": "presses a key: Enter, Tab, Escape, Backspace, Delete, ArrowUp/Down/Left/Right, Home, End, PageUp, PageDown, Space",
            "GET /fill?selector=&value=": "sets a field's value and fires input and change",
            "GET /press?x=&y=": "a real mouse press at a point (moves the user's mouse, brings the app to the front); /click is usually enough",
        })),
        "/page" => {
            let state = STATE.lock().map_err(|_| "busy".to_string())?;
            let tab = state.tabs.get(&active);
            Ok(json!({
                "tab": active,
                "url": tab.map(|t| t.url.clone()),
                "title": tab.map(|t| t.title.clone()),
            }))
        }
        "/console" => {
            let wanted = params.get("level").cloned().unwrap_or_default();
            let state = STATE.lock().map_err(|_| "busy".to_string())?;
            let lines = state
                .tabs
                .get(&active)
                .map(|tab| {
                    tab.console
                        .iter()
                        .filter(|l| {
                            wanted.is_empty()
                                || level_group(l["level"].as_str().unwrap_or_default()) == wanted
                        })
                        .map(|l| json!({ "level": l["level"], "text": l["text"], "at": l["at"] }))
                        .collect()
                })
                .unwrap_or_default();
            Ok(tail(lines, limit))
        }
        "/network" => {
            let failed = params.get("failed").is_some_and(|v| v != "0");
            let state = STATE.lock().map_err(|_| "busy".to_string())?;
            let lines = state
                .tabs
                .get(&active)
                .map(|tab| {
                    tab.network
                        .iter()
                        .filter(|l| {
                            !failed || l["status"].as_i64().is_some_and(|s| s == 0 || s >= 400)
                        })
                        .map(|l| {
                            json!({
                                "method": l["method"], "url": l["url"], "status": l["status"],
                                "ms": l["ms"], "body": l["body"], "at": l["at"],
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(tail(lines, limit))
        }
        "/frames" => {
            let view = page(app, &active)?;
            let frames = crate::cdp::frames(view.label());
            Ok(Value::Array(
                frames
                    .iter()
                    .enumerate()
                    .map(|(index, frame)| json!({ "frame": index, "url": frame.url, "type": frame.kind }))
                    .collect(),
            ))
        }
        "/dom" => {
            if let Some(frame) = params.get("frame") {
                let view = page(app, &active)?;
                let session = frame_session(&view, frame)?;
                let selector = params.get("selector").cloned().unwrap_or_default();
                let script = format!(
                    "(() => {{ const s = {}; const html = s ? [...document.querySelectorAll(s)].slice(0, 10).map((n) => n.outerHTML).join('\\n') : document.documentElement.outerHTML; return {{ url: location.href, title: document.title, html: html.length > 60000 ? html.slice(0, 60000) + '…' : html }}; }})()",
                    json!(selector)
                );
                return crate::cdp::evaluate(&view, &script, Some(session)).await;
            }
            let arg = params.get("selector").map(|s| json!(s)).unwrap_or(Value::Null);
            let report = ask(app, &active, "dom", arg).await?;
            Ok(json!({ "url": report["url"], "title": report["title"], "html": report["html"] }))
        }
        "/shot" => {
            // The picture the page shows now, with a menu open over it laid on top; the file path is the answer.
            let view = page(app, &active)?;
            let mut bytes = crate::cdp::screenshot(&view).await?;
            if let Some(menu) = crate::overlay::visible_view(app) {
                if let Ok(layer) = crate::cdp::transparent_screenshot(&menu).await {
                    let page_at = view.position().map_err(|error| error.to_string())?;
                    let page_size = view.size().map_err(|error| error.to_string())?;
                    let menu_at = menu.position().map_err(|error| error.to_string())?;
                    let (dx, dy) = (menu_at.x - page_at.x, menu_at.y - page_at.y);
                    if let Ok(joined) = compose(&bytes, &layer, dx, dy, page_size.width) {
                        bytes = joined;
                    }
                }
            }
            let path = std::env::temp_dir().join(format!("agentdeck-shot-{}.png", NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
            Ok(json!({ "png": path.display().to_string() }))
        }
        "/click" => {
            let (x, y) = point(params)?;
            let view = page(app, &active)?;
            if crate::cdp::click(&view, x, y).await.is_ok() {
                return Ok(json!({ "clicked": true, "x": x, "y": y }));
            }
            // No DevTools access: the page's own events, dispatched from inside it.
            let report = ask(app, &active, "click", json!({ "x": x, "y": y })).await?;
            match report["error"].as_str() {
                Some(error) => Err(error.to_string()),
                None => Ok(report["result"].clone()),
            }
        }
        "/hover" => {
            let (x, y) = point(params)?;
            crate::cdp::hover(&page(app, &active)?, x, y).await?;
            Ok(json!({ "hovered": true, "x": x, "y": y }))
        }
        "/scroll" => {
            let (x, y) = point(params)?;
            let dy = params.get("dy").and_then(|v| v.parse::<f64>().ok()).unwrap_or(400.0);
            let dx = params.get("dx").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            crate::cdp::scroll(&page(app, &active)?, x, y, dx, dy).await?;
            Ok(json!({ "scrolled": true, "dx": dx, "dy": dy }))
        }
        "/type" => {
            let text = params.get("text").cloned().ok_or("send ?text=")?;
            crate::cdp::type_text(&page(app, &active)?, &text).await?;
            Ok(json!({ "typed": text }))
        }
        "/key" => {
            let key = params.get("key").cloned().ok_or("send ?key=")?;
            crate::cdp::key(&page(app, &active)?, &key).await?;
            Ok(json!({ "pressed": key }))
        }
        "/press" => {
            // A real mouse press at a point of the page (CSS pixels, as in /shot): this moves the user's mouse.
            let x = params
                .get("x")
                .and_then(|v| v.parse::<f64>().ok())
                .ok_or("send ?x=&y= in CSS pixels, as in /shot")?;
            let y = params
                .get("y")
                .and_then(|v| v.parse::<f64>().ok())
                .ok_or("send ?x=&y= in CSS pixels, as in /shot")?;
            // The page's zoom: the width the window shows over the width the page lays out at.
            let report = ask(app, &active, "dom", Value::Null).await?;
            let viewport = report["viewport"]["w"].as_f64().unwrap_or(1.0).max(1.0);
            let view = app
                .get_webview(&format!("browser-{active}"))
                .ok_or("no page is open in the AgentDeck browser")?;
            let logical = view.size().map_err(|error| error.to_string())?;
            let scale = view.window().scale_factor().map_err(|error| error.to_string())?;
            let zoom = (logical.width as f64 / scale) / viewport;
            crate::browser::browser_press(app.clone(), active.clone(), x, y, zoom)?;
            Ok(json!({ "pressed": true, "x": x, "y": y }))
        }
        "/fill" => {
            let selector = params.get("selector").cloned().ok_or("send ?selector=&value=")?;
            let value = params.get("value").cloned().unwrap_or_default();
            let report = ask(app, &active, "fill", json!({ "selector": selector, "value": value })).await?;
            match report["error"].as_str() {
                Some(error) => Err(error.to_string()),
                None => Ok(report["result"].clone()),
            }
        }
        "/eval" => {
            let script = if body.is_empty() {
                params.get("js").cloned().unwrap_or_default()
            } else {
                body.to_string()
            };
            if script.trim().is_empty() {
                return Err("send the JavaScript as the request body (POST) or as ?js=".into());
            }
            let view = page(app, &active)?;
            let session = match params.get("frame") {
                Some(frame) => Some(frame_session(&view, frame)?),
                None => None,
            };
            // Through DevTools first: it works in frames of any origin and on pages whose policy forbids eval.
            match crate::cdp::evaluate(&view, &script, session.clone()).await {
                Ok(value) => Ok(json!({ "result": value })),
                Err(error) if session.is_some() => Err(error),
                Err(_) => {
                    let report = ask(app, &active, "eval", json!(script)).await?;
                    match report["error"].as_str() {
                        Some(error) => Err(error.to_string()),
                        None => Ok(json!({ "result": report["result"] })),
                    }
                }
            }
        }
        _ => Err("unknown path; GET /help lists them".into()),
    }
}

/// The webview of a tab's page.
fn page(app: &AppHandle, tab: &str) -> Result<tauri::Webview, String> {
    if tab.is_empty() {
        return Err("no page is open in the AgentDeck browser".into());
    }
    app.get_webview(&format!("browser-{tab}"))
        .ok_or_else(|| "no page is open in the AgentDeck browser".to_string())
}

/// `?x=&y=`, in CSS pixels of the page.
fn point(params: &HashMap<String, String>) -> Result<(f64, f64), String> {
    let read = |key: &str| params.get(key).and_then(|v| v.parse::<f64>().ok());
    match (read("x"), read("y")) {
        (Some(x), Some(y)) => Ok((x, y)),
        _ => Err("send ?x=&y= in CSS pixels of the page, as in /shot".into()),
    }
}

/// A frame chosen by its index in /frames or by a part of its address.
fn frame_session(view: &tauri::Webview, frame: &str) -> Result<String, String> {
    let frames = crate::cdp::frames(view.label());
    // An index in the list, else a part of the address (a port such as 8766 is an address, not an index).
    let found = frame
        .parse::<usize>()
        .ok()
        .and_then(|index| frames.get(index))
        .or_else(|| frames.iter().find(|item| item.url.contains(frame)));
    found
        .map(|item| item.session.clone())
        .ok_or_else(|| format!("no frame matches {frame}; GET /frames lists them"))
}

/// Lays the menu overlay's picture over the page's, at its offset (physical pixels of the window). The page's
/// picture may be at another scale than the window (a zoomed phone page); the offset follows that scale.
fn compose(page: &[u8], layer: &[u8], dx: i32, dy: i32, page_width: u32) -> Result<Vec<u8>, String> {
    let (mut base, width, height) = rgba(page)?;
    let (top, top_width, top_height) = rgba(layer)?;
    let ratio = width as f64 / page_width.max(1) as f64;
    for row in 0..(top_height as f64 * ratio) as u32 {
        for col in 0..(top_width as f64 * ratio) as u32 {
            let x = (dx as f64 * ratio) as i64 + col as i64;
            let y = (dy as f64 * ratio) as i64 + row as i64;
            if x < 0 || y < 0 || x >= width as i64 || y >= height as i64 {
                continue;
            }
            let sx = ((col as f64 / ratio) as u32).min(top_width - 1);
            let sy = ((row as f64 / ratio) as u32).min(top_height - 1);
            let from = ((sy * top_width + sx) * 4) as usize;
            let to = ((y as u32 * width + x as u32) * 4) as usize;
            let alpha = top[from + 3] as u32;
            for channel in 0..3 {
                let over = top[from + channel] as u32;
                let under = base[to + channel] as u32;
                base[to + channel] = ((over * alpha + under * (255 - alpha)) / 255) as u8;
            }
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer.write_image_data(&base).map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())?;
    Ok(out)
}

/// A PNG as 8-bit RGBA pixels.
fn rgba(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buffer).map_err(|error| error.to_string())?;
    let pixels = &buffer[..info.buffer_size()];
    let out = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => pixels.chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err("indexed PNG".into()),
    };
    Ok((out, info.width, info.height))
}

/// Asks the page of a tab for something and waits for its answer.
async fn ask(app: &AppHandle, tab: &str, kind: &str, arg: Value) -> Result<Value, String> {
    if tab.is_empty() {
        return Err("no page is open in the AgentDeck browser".into());
    }
    let id = format!("bridge-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let (sender, receiver) = oneshot::channel();
    STATE
        .lock()
        .map_err(|_| "busy".to_string())?
        .waiting
        .insert(id.clone(), sender);
    let view = app
        .get_webview(&format!("browser-{tab}"))
        .ok_or("no page is open in the AgentDeck browser")?;
    let script = format!(
        "window.__agentdeck && window.__agentdeck.run({}, {}, {})",
        json!(id),
        json!(kind),
        arg
    );
    view.eval(script).map_err(|e| e.to_string())?;
    match tokio::time::timeout(Duration::from_secs(15), receiver).await {
        Ok(Ok(report)) => Ok(report),
        _ => {
            if let Ok(mut state) = STATE.lock() {
                state.waiting.remove(&id);
            }
            Err("the page did not answer".into())
        }
    }
}
