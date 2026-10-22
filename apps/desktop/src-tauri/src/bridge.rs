//! A small HTTP server on 127.0.0.1 that lets the agent read what the AgentDeck browser shows: the console, the
//! network, the DOM, and run a script in the page. The address is put in `AGENTDECK_BRIDGE` (the agent process
//! inherits it) and named in the agent system prompt. It only listens on the loopback interface.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use base64::Engine;
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

async fn answer(
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
            "GET /dom?selector=css": "outerHTML of the page, or of the elements matching a selector",
            "POST /eval (body: JavaScript)": "runs the script in the page and returns its result",
            "GET /shot": "a PNG of the page as shown now; answers with the file path",
            "GET /click?x=&y=": "presses at a point of the page (CSS pixels, as in /shot), with the events of a real press",
            "GET /fill?selector=&value=": "types a value into a field and fires input and change",
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
        "/dom" => {
            let arg = params.get("selector").map(|s| json!(s)).unwrap_or(Value::Null);
            let report = ask(app, &active, "dom", arg).await?;
            Ok(json!({ "url": report["url"], "title": report["title"], "html": report["html"] }))
        }
        "/shot" => {
            // The picture the page shows now, written to a file; the path is the answer.
            let data = crate::browser::browser_capture(app.clone(), active.clone()).await?;
            let encoded = data.split_once(',').map(|(_, rest)| rest).unwrap_or_default();
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| error.to_string())?;
            let path = std::env::temp_dir().join(format!("agentdeck-shot-{}.png", NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
            Ok(json!({ "png": path.display().to_string() }))
        }
        "/click" => {
            let x = params.get("x").and_then(|v| v.parse::<f64>().ok()).ok_or("send ?x=&y= in CSS pixels, as in /shot")?;
            let y = params.get("y").and_then(|v| v.parse::<f64>().ok()).ok_or("send ?x=&y= in CSS pixels, as in /shot")?;
            let report = ask(app, &active, "click", json!({ "x": x, "y": y })).await?;
            match report["error"].as_str() {
                Some(error) => Err(error.to_string()),
                None => Ok(report["result"].clone()),
            }
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
            let report = ask(app, &active, "eval", json!(script)).await?;
            match report["error"].as_str() {
                Some(error) => Err(error.to_string()),
                None => Ok(json!({ "result": report["result"] })),
            }
        }
        _ => Err("unknown path; GET /help lists them".into()),
    }
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
