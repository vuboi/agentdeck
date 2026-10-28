//! A small HTTP server on 127.0.0.1 that lets the agent read what the AgentDeck browser shows: the console, the
//! network, the DOM, and run a script in the page. The address is put in `AGENTDECK_BRIDGE` (the agent process
//! inherits it) and named in the agent system prompt. It only listens on the loopback interface.
//!
//! A web page in the user's browser can also reach 127.0.0.1, so the server does not answer just anyone: every
//! address starts with a secret token (`AGENTDECK_BRIDGE` already includes it), the Host must be the loopback
//! address, and a request that carries an `Origin` header (a browser page, never an agent) is refused.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU16, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
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
    /// Which chat last opened a page in each tab: a tab driven by one chat is not taken over by another.
    owners: HashMap<String, String>,
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

/// The tab the agent's requests are about (the one last shown).
pub fn active_tab() -> String {
    STATE
        .lock()
        .map(|state| state.active.clone())
        .unwrap_or_default()
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
        "console" => push(
            &mut state.tabs.entry(tab).or_default().console,
            report.clone(),
        ),
        "network" => push(
            &mut state.tabs.entry(tab).or_default().network,
            report.clone(),
        ),
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

const PREFERRED_PORT: u16 = 47821;

/// The secret every address of the bridge starts with. It is kept in the app's data folder, so the address
/// stays the same between runs (an MCP server stored in an agent's own config keeps working).
static TOKEN: OnceLock<String> = OnceLock::new();
static PORT: AtomicU16 = AtomicU16::new(0);

fn load_token(app: &AppHandle) -> Option<String> {
    let file = app
        .path()
        .app_data_dir()
        .ok()
        .map(|dir| dir.join("bridge-token"));
    if let Some(text) = file
        .as_ref()
        .and_then(|file| std::fs::read_to_string(file).ok())
    {
        let text = text.trim();
        if text.len() >= 32 && text.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(text.to_string());
        }
    }
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).ok()?;
    let token: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    if let Some(file) = file {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&file, &token).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    Some(token)
}

/// The target of a request without the token, or None when the request must be refused: wrong or missing token,
/// a Host that is not this machine's loopback address (DNS rebinding), or an Origin header (a web page).
fn authorize(head: &str, target: &str, token: &str, port: u16) -> Option<String> {
    let mut host = None;
    for line in head.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "origin" => return None,
            "host" => host = Some(value.trim().to_ascii_lowercase()),
            _ => {}
        }
    }
    let host = host?;
    if host != format!("127.0.0.1:{port}") && host != format!("localhost:{port}") {
        return None;
    }
    let rest = target.strip_prefix('/')?.strip_prefix(token)?;
    match rest.chars().next() {
        None => Some("/".to_string()),
        Some('/') => Some(rest.to_string()),
        Some('?') => Some(format!("/{rest}")),
        Some(_) => None,
    }
}

/// Keep stable bridge ports separate across installed, development and local release channels.
fn preferred_port(identifier: &str) -> u16 {
    match identifier {
        "dev.agentdeck.desktop.dev" => PREFERRED_PORT + 1,
        "dev.agentdeck.desktop.local" => PREFERRED_PORT + 2,
        _ => PREFERRED_PORT,
    }
}

/// Starts the server; its address goes to `AGENTDECK_BRIDGE`.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // A fixed port first: an MCP server stored in Antigravity's own config (it has no per-turn flag) must keep
        // its address between runs. If another program has it, any free port does and the MCP tab shows the link as stale.
        let preferred = preferred_port(&app.config().identifier);
        let bound = match TcpListener::bind(("127.0.0.1", preferred)).await {
            Ok(listener) => Ok(listener),
            Err(_) => TcpListener::bind("127.0.0.1:0").await,
        };
        let Ok(listener) = bound else {
            return;
        };
        let Ok(address) = listener.local_addr() else {
            return;
        };
        let Some(token) = load_token(&app) else {
            return;
        };
        let _ = TOKEN.set(token.clone());
        PORT.store(address.port(), Ordering::Relaxed);
        std::env::set_var("AGENTDECK_BRIDGE", format!("http://{address}/{token}"));
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
    let Some(target) = TOKEN
        .get()
        .and_then(|token| authorize(&head, &target, token, PORT.load(Ordering::Relaxed)))
    else {
        let refusal = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        stream.write_all(refusal.as_bytes()).await?;
        return stream.shutdown().await;
    };
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
        let reply = if is_post_request(&head) {
            let chat = params.get("chat").cloned().unwrap_or_default();
            if let Some(root) = params.get("root") {
                crate::index::set_root(&chat, root);
            }
            // A call that waits for another agent for minutes (delegate_task) answers as an event stream with a
            // comment every few seconds: a client takes a response that stays silent for five minutes as dead.
            if accepts_stream(&head) && crate::delegate::is_long_call(&body) {
                return stream_answer(stream, &app, &body, &chat).await;
            }
            match crate::mcp::handle(&app, &body, &chat).await {
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

"
                .to_string(),
            }
        } else {
            "HTTP/1.1 405 Method Not Allowed
Allow: POST
Content-Length: 0
Connection: close

"
            .to_string()
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

/// Whether the client takes the answer to a POST as a `text/event-stream` (MCP clients send both kinds in `Accept`).
fn accepts_stream(head: &str) -> bool {
    head.lines().skip(1).any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.trim().eq_ignore_ascii_case("accept")
                && value.to_ascii_lowercase().contains("text/event-stream")
        })
    })
}

/// Answers a JSON-RPC request as a server-sent event: the headers go out at once, `: keepalive` comments follow while
/// the call works, and the answer ends the stream. A client that went away makes a write fail, which drops the call.
async fn stream_answer(
    mut stream: TcpStream,
    app: &AppHandle,
    body: &str,
    chat: &str,
) -> std::io::Result<()> {
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n: open\n\n";
    stream.write_all(head.as_bytes()).await?;
    let work = crate::mcp::handle(app, body, chat);
    tokio::pin!(work);
    let mut beat = tokio::time::interval(Duration::from_secs(15));
    beat.tick().await;
    let answer = loop {
        tokio::select! {
            answer = &mut work => break answer,
            _ = beat.tick() => stream.write_all(b": keepalive\n\n").await?,
        }
    };
    if let Some(text) = answer {
        stream
            .write_all(format!("event: message\ndata: {text}\n\n").as_bytes())
            .await?;
    }
    stream.shutdown().await
}

fn is_post_request(head: &str) -> bool {
    head.lines()
        .next()
        .is_some_and(|line| line.to_ascii_lowercase().starts_with("post "))
}

fn decode(text: &str) -> String {
    let bytes = text.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&String::from_utf8_lossy(&bytes[i + 1..i + 3]), 16)
            {
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

/// Opens `url` in the Preview panel (a tab of its own when another page is open there) and waits until the page
/// answers the injected script: then it is ready to act on.
async fn open_page(app: &AppHandle, url: &str, new_tab: bool, chat: &str) -> Result<Value, String> {
    app.emit("agent-open", json!({ "url": url, "newTab": new_tab }))
        .map_err(|error| error.to_string())?;
    let wanted = url.trim_end_matches('/').to_string();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let tab = STATE.lock().map_err(|_| "busy".to_string())?.active.clone();
        if tab.is_empty() {
            continue;
        }
        if let Ok(report) = ask(app, &tab, "dom", Value::Null).await {
            let shown = report["url"].as_str().unwrap_or_default();
            if shown.trim_end_matches('/').starts_with(&wanted) {
                if !chat.is_empty() {
                    STATE
                        .lock()
                        .map_err(|_| "busy".to_string())?
                        .owners
                        .insert(tab.clone(), chat.to_string());
                }
                return Ok(
                    json!({ "opened": shown, "title": report["title"], "tab": tab, "newTab": new_tab }),
                );
            }
        }
    }
    Err(format!("{url} did not open in Preview within 30 s"))
}

/// Whether a page the agent opens goes to a tab of its own. `new_tab` says it outright. Otherwise the front tab is
/// used, unless it shows a page that another chat opened and drives: that one is left alone.
fn wants_new_tab(params: &HashMap<String, String>) -> bool {
    if let Some(value) = params.get("new_tab") {
        return value == "1" || value == "true";
    }
    let chat = params.get("chat").map_or("", String::as_str);
    let Ok(state) = STATE.lock() else {
        return false;
    };
    let shows_a_page = state
        .tabs
        .get(&state.active)
        .is_some_and(|tab| !tab.url.is_empty());
    shows_a_page
        && state
            .owners
            .get(&state.active)
            .is_some_and(|owner| !chat.is_empty() && owner != chat)
}

/// The MIME type of a file, from its extension, for the file the agent uploads.
fn mime_of(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "txt" | "md" => "text/plain",
        "csv" => "text/csv",
        "json" => "application/json",
        "html" => "text/html",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
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
            "GET /serve?command=&cwd=&url=": "starts a dev server owned by AgentDeck (it outlives your turn), waits until the address answers, then opens it",
            "GET /device?mode=desktop|tablet|mobile&landscape=1": "switches the Preview to a device (the page is created again with its user agent and size) and answers with the viewport",
            "GET /open?url=":"opens an address in the Preview panel and waits until the page is ready to act on",
            "GET /drag?from=&to=":"drags one element onto another (HTML5 drag and drop), both as CSS selectors",
            "GET /upload?selector=&path=": "uploads a file from disk: set on an input type=file, or dropped on a drop zone",
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
            // failed=1: only 0 and >= 400. ok=1: only the successful ones. url=: only urls containing it.
            // bodies=1: responses and what was sent too (to write a DTO); max_body= characters per response.
            let failed = params.get("failed").is_some_and(|v| v != "0");
            let only_ok = params.get("ok").is_some_and(|v| v != "0");
            let bodies = params.get("bodies").is_some_and(|v| v != "0");
            let url_filter = params.get("url").cloned();
            let max_body = params
                .get("max_body")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(2000)
                .min(20000);
            let state = STATE.lock().map_err(|_| "busy".to_string())?;
            let lines = state
                .tabs
                .get(&active)
                .map(|tab| {
                    tab.network
                        .iter()
                        .filter(|l| {
                            let bad = l["status"].as_i64().is_some_and(|s| s == 0 || s >= 400);
                            (!failed || bad)
                                && (!only_ok || !bad)
                                && url_filter.as_deref().is_none_or(|part| l["url"].as_str().is_some_and(|url| url.contains(part)))
                        })
                        .map(|l| {
                            let bad = l["status"].as_i64().is_some_and(|s| s == 0 || s >= 400);
                            let clip = |value: &Value, max: usize| match value.as_str() {
                                Some(text) if text.chars().count() > max => json!(format!("{}…", text.chars().take(max).collect::<String>())),
                                _ => value.clone(),
                            };
                            json!({
                                "method": l["method"], "url": l["url"], "status": l["status"],
                                "ms": l["ms"], "at": l["at"],
                                "request": if bodies { clip(&l["request"], max_body) } else { Value::Null },
                                "body": if bodies || bad { clip(&l["body"], max_body) } else { Value::Null },
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
            let arg = params
                .get("selector")
                .map(|s| json!(s))
                .unwrap_or(Value::Null);
            let report = ask(app, &active, "dom", arg).await?;
            Ok(json!({ "url": report["url"], "title": report["title"], "html": report["html"] }))
        }
        "/shot" => {
            // The picture the page shows now; the file path is the answer.
            let view = page(app, &active)?;
            let bytes = match crate::cdp::screenshot(&view).await {
                Ok(bytes) => bytes,
                // No DevTools access (macOS, Linux): the page draws itself.
                Err(_) => shot_via_page(app, &active).await?,
            };
            let path = std::env::temp_dir().join(format!(
                "agentdeck-shot-{}.png",
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
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
            if crate::cdp::hover(&page(app, &active)?, x, y).await.is_err() {
                act_via_page(app, &active, "hover", json!({ "x": x, "y": y })).await?;
            }
            Ok(json!({ "hovered": true, "x": x, "y": y }))
        }
        "/scroll" => {
            let (x, y) = point(params)?;
            let dy = params
                .get("dy")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(400.0);
            let dx = params
                .get("dx")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            if crate::cdp::scroll(&page(app, &active)?, x, y, dx, dy)
                .await
                .is_err()
            {
                act_via_page(
                    app,
                    &active,
                    "scroll",
                    json!({ "x": x, "y": y, "dx": dx, "dy": dy }),
                )
                .await?;
            }
            Ok(json!({ "scrolled": true, "dx": dx, "dy": dy }))
        }
        "/type" => {
            let text = params.get("text").cloned().ok_or("send ?text=")?;
            if crate::cdp::type_text(&page(app, &active)?, &text)
                .await
                .is_err()
            {
                act_via_page(app, &active, "type", json!(text)).await?;
            }
            Ok(json!({ "typed": text }))
        }
        "/key" => {
            let key = params.get("key").cloned().ok_or("send ?key=")?;
            if crate::cdp::key(&page(app, &active)?, &key).await.is_err() {
                act_via_page(app, &active, "key", json!(key)).await?;
            }
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
                .ok_or("no page is open in the AgentDeck browser: call browser_open (or browser_serve to start the server) with the page address first")?;
            let logical = view.size().map_err(|error| error.to_string())?;
            let scale = view
                .window()
                .scale_factor()
                .map_err(|error| error.to_string())?;
            let zoom = (logical.width as f64 / scale) / viewport;
            crate::browser::browser_press(app.clone(), active.clone(), x, y, zoom)?;
            Ok(json!({ "pressed": true, "x": x, "y": y }))
        }
        "/fill" => {
            let selector = params
                .get("selector")
                .cloned()
                .ok_or("send ?selector=&value=")?;
            let value = params.get("value").cloned().unwrap_or_default();
            let report = ask(
                app,
                &active,
                "fill",
                json!({ "selector": selector, "value": value }),
            )
            .await?;
            match report["error"].as_str() {
                Some(error) => Err(error.to_string()),
                None => Ok(report["result"].clone()),
            }
        }
        "/drag" => {
            let from = params
                .get("from")
                .cloned()
                .ok_or("send ?from=&to= as CSS selectors")?;
            let to = params
                .get("to")
                .cloned()
                .ok_or("send ?from=&to= as CSS selectors")?;
            let report = ask(app, &active, "drag", json!({ "from": from, "to": to })).await?;
            match report["error"].as_str() {
                Some(error) => Err(error.to_string()),
                None => Ok(report["result"].clone()),
            }
        }
        "/upload" => {
            let selector = params
                .get("selector")
                .cloned()
                .ok_or("send ?selector=&path=")?;
            let path = params
                .get("path")
                .cloned()
                .ok_or("send ?selector=&path= with an absolute file path")?;
            let bytes = std::fs::read(&path).map_err(|error| format!("{path}: {error}"))?;
            if bytes.len() > 25 * 1024 * 1024 {
                return Err(format!("{path} is over 25 MB"));
            }
            let name = std::path::Path::new(&path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("file")
                .to_string();
            use base64::Engine;
            let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let mime = mime_of(&name);
            let report = ask(
                app,
                &active,
                "upload",
                json!({ "selector": selector, "name": name, "type": mime, "data": data }),
            )
            .await?;
            match report["error"].as_str() {
                Some(error) => Err(error.to_string()),
                None => Ok(report["result"].clone()),
            }
        }
        "/open" => {
            let url = params
                .get("url")
                .cloned()
                .ok_or("send ?url= with the address to open")?;
            let target = crate::browser::parse(&url)?;
            // The address that answers, at the scheme the server speaks (a server with a certificate is https only).
            let Some(target) = crate::browser::ready(&target).await else {
                return Err(format!(
                    "nothing answers at {target}: start the server with browser_serve first"
                ));
            };
            let new_tab = wants_new_tab(params);
            open_page(
                app,
                target.as_str(),
                new_tab,
                params.get("chat").map_or("", String::as_str),
            )
            .await
        }
        "/device" => {
            let mode = params
                .get("mode")
                .cloned()
                .ok_or("send ?mode=desktop|tablet|mobile")?;
            if !["desktop", "tablet", "mobile"].contains(&mode.as_str()) {
                return Err(format!(
                    "unknown device {mode}: use desktop, tablet or mobile"
                ));
            }
            let landscape = params
                .get("landscape")
                .is_some_and(|value| value == "1" || value == "true");
            app.emit(
                "agent-device",
                json!({ "device": mode, "landscape": landscape }),
            )
            .map_err(|error| error.to_string())?;
            // The pages are created again with the device's user agent: wait for the new one to answer.
            tokio::time::sleep(Duration::from_millis(1500)).await;
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            while std::time::Instant::now() < deadline {
                let tab = STATE.lock().map_err(|_| "busy".to_string())?.active.clone();
                if !tab.is_empty() {
                    if let Ok(report) = ask(app, &tab, "dom", Value::Null).await {
                        return Ok(
                            json!({ "device": mode, "landscape": landscape, "viewport": report["viewport"], "url": report["url"] }),
                        );
                    }
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err("the page did not come back after the device change".into())
        }
        "/serve" => {
            let command = params
                .get("command")
                .cloned()
                .ok_or("send ?command=&cwd=&url=")?;
            let cwd = params
                .get("cwd")
                .cloned()
                .ok_or("send ?cwd= with the project folder")?;
            let url = params
                .get("url")
                .cloned()
                .ok_or("send ?url= with the address the server will answer at")?;
            let guess = crate::browser::parse(&url)?;
            // What to answer when the start script asks questions, in order, separated by `|`.
            let answers: Vec<String> = params
                .get("answers")
                .filter(|text| !text.is_empty())
                .map(|text| text.split('|').map(|answer| answer.trim().to_string()).collect())
                .unwrap_or_default();
            let (target, log) = crate::detect::bring_up(&command, &cwd, Some(guess), &answers)
                .await
                .map_err(|failure| format!("`{command}` did not start a server at {url}. {}", failure.text()))?;
            let new_tab = wants_new_tab(params);
            let mut opened = open_page(
                app,
                target.as_str(),
                new_tab,
                params.get("chat").map_or("", String::as_str),
            )
            .await?;
            opened["server"] = json!({ "command": command, "log": log.map(|path| path.to_string_lossy().into_owned()) });
            Ok(opened)
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
            // A script that waits or polls (an animation, a page settling) may ask for more time, up to a minute.
            let seconds = params
                .get("timeout")
                .and_then(|value| value.parse::<f64>().ok())
                .map_or(20, |value| value.clamp(1.0, 60.0).ceil() as u64);
            match crate::cdp::evaluate_for(&view, &script, session.clone(), seconds).await {
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
        return Err("no page is open in the AgentDeck browser: call browser_open (or browser_serve to start the server) with the page address first".into());
    }
    app.get_webview(&format!("browser-{tab}"))
        .ok_or_else(|| "no page is open in the AgentDeck browser: call browser_open (or browser_serve to start the server) with the page address first".to_string())
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

/// The page drawing itself, PNG bytes: for platforms where DevTools cannot take the picture.
async fn shot_via_page(app: &AppHandle, tab: &str) -> Result<Vec<u8>, String> {
    use base64::Engine;
    let report = ask(app, tab, "shot", Value::Null).await?;
    if let Some(error) = report["error"].as_str() {
        return Err(error.to_string());
    }
    let data = report["data"].as_str().ok_or("no screenshot data")?;
    let encoded = data.split_once(',').map_or(data, |(_, rest)| rest);
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| error.to_string())
}

/// An action done by the page itself (its own events), for platforms where DevTools cannot do it.
async fn act_via_page(app: &AppHandle, tab: &str, kind: &str, arg: Value) -> Result<Value, String> {
    let report = ask(app, tab, kind, arg).await?;
    match report["error"].as_str() {
        Some(error) => Err(error.to_string()),
        None => Ok(report["result"].clone()),
    }
}

/// Asks the page of a tab for something and waits for its answer.
pub(crate) async fn ask(
    app: &AppHandle,
    tab: &str,
    kind: &str,
    arg: Value,
) -> Result<Value, String> {
    if tab.is_empty() {
        return Err("no page is open in the AgentDeck browser: call browser_open (or browser_serve to start the server) with the page address first".into());
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
        .ok_or("no page is open in the AgentDeck browser: call browser_open (or browser_serve to start the server) with the page address first")?;
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

#[cfg(test)]
mod tests {
    use super::{authorize, is_post_request};

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn head(target: &str, extra: &str) -> String {
        format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:47821\r\n{extra}\r\n")
    }

    #[test]
    fn token_in_the_path_is_stripped() {
        let target = format!("/{TOKEN}/page?x=1");
        assert_eq!(
            authorize(&head(&target, ""), &target, TOKEN, 47821).as_deref(),
            Some("/page?x=1")
        );
        let bare = format!("/{TOKEN}");
        assert_eq!(
            authorize(&head(&bare, ""), &bare, TOKEN, 47821).as_deref(),
            Some("/")
        );
    }

    #[test]
    fn wrong_or_missing_token_is_refused() {
        assert!(authorize(&head("/page", ""), "/page", TOKEN, 47821).is_none());
        let wrong = format!("/{}x/page", TOKEN);
        assert!(authorize(&head(&wrong, ""), &wrong, TOKEN, 47821).is_none());
        assert!(authorize(&head("/mcp", ""), "/mcp", TOKEN, 47821).is_none());
    }

    #[test]
    fn foreign_host_or_origin_is_refused() {
        let target = format!("/{TOKEN}/page");
        let rebound = "GET / HTTP/1.1\r\nHost: evil.example:47821\r\n\r\n";
        assert!(authorize(rebound, &target, TOKEN, 47821).is_none());
        assert!(authorize(
            &head(&target, "Origin: https://evil.example\r\n"),
            &target,
            TOKEN,
            47821
        )
        .is_none());
        assert!(authorize("GET / HTTP/1.1\r\n\r\n", &target, TOKEN, 47821).is_none());
        let local = "GET / HTTP/1.1\r\nHost: localhost:47821\r\n\r\n";
        assert!(authorize(local, &target, TOKEN, 47821).is_some());
    }

    #[test]
    fn accepts_post_method_regardless_of_case() {
        assert!(is_post_request("POST /mcp HTTP/1.1\r\n"));
        assert!(is_post_request("post /mcp HTTP/1.1\r\n"));
        assert!(is_post_request("PoSt /mcp HTTP/1.1\r\n"));
    }

    #[test]
    fn a_stream_is_sent_only_to_a_client_that_accepts_one() {
        use super::accepts_stream;
        assert!(accepts_stream(
            "POST /mcp HTTP/1.1\r\nHost: x\r\nAccept: application/json, text/event-stream\r\n"
        ));
        assert!(accepts_stream(
            "POST /mcp HTTP/1.1\r\naccept: Text/Event-Stream\r\n"
        ));
        assert!(!accepts_stream(
            "POST /mcp HTTP/1.1\r\nAccept: application/json\r\n"
        ));
        assert!(!accepts_stream("POST /mcp HTTP/1.1\r\nHost: x\r\n"));
    }

    #[test]
    fn rejects_other_methods_and_prefixes() {
        assert!(!is_post_request("GET /mcp HTTP/1.1\r\n"));
        assert!(!is_post_request("POSTX /mcp HTTP/1.1\r\n"));
        assert!(!is_post_request("POST"));
    }
}

#[cfg(test)]
mod channel_tests {
    use super::preferred_port;
    #[test]
    fn installed_dev_and_local_have_distinct_bridge_ports() {
        assert_eq!(preferred_port("dev.agentdeck.desktop"), 47821);
        assert_eq!(preferred_port("dev.agentdeck.desktop.dev"), 47822);
        assert_eq!(preferred_port("dev.agentdeck.desktop.local"), 47823);
    }
}
