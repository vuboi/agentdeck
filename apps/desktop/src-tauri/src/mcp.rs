//! The AgentDeck browser as an MCP server (streamable HTTP, JSON answers), served by the bridge at `/mcp`.
//!
//! The agent CLI is started with it in `--mcp-config`, so the browser shows up among its own tools
//! (`mcp__agentdeck__browser_click`, ...) instead of as curl commands described in the prompt. Each tool is a
//! bridge route: the work itself stays in `bridge.rs`.

use std::collections::HashMap;

use serde_json::{json, Value};
use tauri::AppHandle;

struct Tool {
    name: &'static str,
    route: &'static str,
    description: &'static str,
    /// (name, JSON type, description, required)
    args: &'static [(&'static str, &'static str, &'static str, bool)],
}

const TOOLS: &[Tool] = &[
    Tool { name: "browser_page", route: "/page", description: "Address and title of the page open in AgentDeck's Preview panel (the browser the user sees).", args: &[] },
    Tool { name: "browser_screenshot", route: "/shot", description: "A picture of the Preview page as the user sees it now, menus included. Look before and after every action.", args: &[] },
    Tool { name: "browser_click", route: "/click", description: "Clicks at a point of the page, in CSS pixels of the screenshot. Real mouse events; reaches iframes.", args: &[("x", "number", "from the left of the page", true), ("y", "number", "from the top of the page", true)] },
    Tool { name: "browser_hover", route: "/hover", description: "Moves the mouse over a point (hover menus, tooltips).", args: &[("x", "number", "", true), ("y", "number", "", true)] },
    Tool { name: "browser_scroll", route: "/scroll", description: "Scrolls at a point, like a mouse wheel. dy > 0 scrolls down.", args: &[("x", "number", "", true), ("y", "number", "", true), ("dy", "number", "pixels down (negative: up)", false), ("dx", "number", "pixels right", false)] },
    Tool { name: "browser_type", route: "/type", description: "Types text into the focused field, like a keyboard. Click the field first.", args: &[("text", "string", "", true)] },
    Tool { name: "browser_key", route: "/key", description: "Presses a key: Enter, Tab, Escape, Backspace, Delete, ArrowUp/Down/Left/Right, Home, End, PageUp, PageDown, Space.", args: &[("key", "string", "", true)] },
    Tool { name: "browser_fill", route: "/fill", description: "Sets the value of the field matching a CSS selector and fires input and change.", args: &[("selector", "string", "", true), ("value", "string", "", true)] },
    Tool { name: "browser_device", route: "/device", description: "Switches the Preview between desktop, tablet and mobile (a phone or tablet user agent and screen size), to check a responsive layout. Answers with the viewport after the switch.", args: &[("mode", "string", "desktop, tablet or mobile", true), ("landscape", "boolean", "rotate the device", false)] },
    Tool { name: "browser_serve", route: "/serve", description: "Starts a local dev server that AgentDeck owns, waits until its address answers, then opens it in the Preview panel. Use it for every dev server: one started in your own shell dies when your command or turn ends. If the address already answers, it only opens it. A failed start returns the end of the server's log.", args: &[("command", "string", "the start command, for example pnpm start:svelte", true), ("cwd", "string", "absolute path of the project folder", true), ("url", "string", "the address the server answers at, for example http://localhost:4301/", true), ("new_tab", "boolean", "true: open it in a tab of its own, leaving the current tab as it is; false: use the current tab. Left out: the current tab, unless another chat is driving it.", false)] },
    Tool { name: "browser_open", route: "/open", description: "Opens an address in AgentDeck's Preview panel (a tab of its own when another page is open) and waits until the page is ready. Use it right after starting a dev server, then screenshot and act in the same turn.", args: &[("url", "string", "full address, for example http://localhost:4301/", true), ("new_tab", "boolean", "true: open it in a tab of its own, leaving the current tab as it is (use it when another chat is driving the current tab, to compare, to debug side by side, or to look something up). false: use the current tab. Left out: the current tab, unless another chat is driving it.", false)] },
    Tool { name: "browser_drag", route: "/drag", description: "Drags one element onto another, as HTML5 drag and drop (dragstart, dragover, drop). Both are CSS selectors.", args: &[("from", "string", "CSS selector of the element to drag", true), ("to", "string", "CSS selector of the element to drop on", true)] },
    Tool { name: "browser_upload", route: "/upload", description: "Uploads a file from disk: sets it on an <input type=file>, or drops it on a drop zone. Give the absolute file path.", args: &[("selector", "string", "CSS selector of the file input or drop zone", true), ("path", "string", "absolute path of the file", true)] },
    Tool { name: "browser_eval", route: "/eval", description: "Runs JavaScript in the page (or in a frame, see browser_frames) and returns its value. Use it to read state or find an element's position with getBoundingClientRect().", args: &[("script", "string", "an expression; promises are awaited", true), ("frame", "string", "frame index or part of its address", false)] },
    Tool { name: "browser_dom", route: "/dom", description: "HTML of the page, or of the elements matching a CSS selector.", args: &[("selector", "string", "", false), ("frame", "string", "", false)] },
    Tool { name: "browser_console", route: "/console", description: "Console lines and uncaught JavaScript errors of the page.", args: &[("level", "string", "error, warn or info", false), ("limit", "number", "", false)] },
    Tool { name: "browser_network", route: "/network", description: "Requests of the page; failed ones carry the start of the response body. With bodies, every request carries what was sent and the response (JSON, text), to write a DTO or an interface from a real response, or to see why a call failed.", args: &[("failed", "boolean", "only failed requests (status 0 or >= 400)", false), ("ok", "boolean", "only successful requests", false), ("url", "string", "only requests whose address contains this text, for example /api/users", false), ("bodies", "boolean", "include the request body and the response body", false), ("max_body", "number", "characters kept per body (default 2000, at most 20000)", false), ("limit", "number", "", false)] },
    Tool { name: "browser_frames", route: "/frames", description: "Frames of other origins in the page (sign-in, payment, embeds).", args: &[] },
    Tool { name: "browser_elements", route: "/eval", description: "Lists what can be clicked or filled in the visible part of the page (buttons, links, fields, menus), each with a number, its visible text and its position. Use the number with browser_click_element. Scroll with browser_scroll and call it again to see more. This is the reliable way to find things: prefer it to guessing coordinates from a screenshot.", args: &[] },
    Tool { name: "browser_click_element", route: "/click", description: "Clicks an element: by the number from browser_elements, or by its visible text, or by a CSS selector. It scrolls the element into view and clicks its centre with real mouse events. For a field, click it first, then browser_type.", args: &[("index", "number", "number from browser_elements", false), ("text", "string", "visible text or label of the element", false), ("selector", "string", "CSS selector", false)] },
];

/// Page script shared by the element tools: finds the open dialog, drawer or modal (if any), and names elements.
const PAGE_HELPERS: &str = r#"
const adShown = (e) => { const r = e.getBoundingClientRect(); const cs = getComputedStyle(e); return r.width > 20 && r.height > 20 && cs.visibility !== 'hidden' && cs.display !== 'none' && +cs.opacity > 0.05; };
const adOverlay = () => {
  const found = [...document.querySelectorAll('[role=dialog],[role=alertdialog],[aria-modal=true],dialog[open],[class*=drawer],[class*=modal],[class*=sheet],[class*=overlay],[class*=popover],[class*=dropdown-menu],[class*=menu-panel]')]
    .filter((e) => adShown(e) && /^(fixed|absolute|sticky)$/.test(getComputedStyle(e).position))
    .filter((e) => { const r = e.getBoundingClientRect(); return r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth; });
  const z = (e) => { let n = 0; for (let p = e; p && p !== document.body; p = p.parentElement) { const v = parseInt(getComputedStyle(p).zIndex, 10); if (!isNaN(v)) n = Math.max(n, v); } return n; };
  const roots = found.filter((e) => !found.some((o) => o !== e && o.contains(e)));
  roots.sort((a, b) => z(b) - z(a));
  return roots.find((e) => e.matches('[role=dialog],[role=alertdialog],[aria-modal=true],dialog[open]')) || roots[0] || null;
};
const adName = (el) => {
  const text = (el.getAttribute('aria-label') || el.innerText || el.value || el.placeholder || el.title || el.alt || el.name || el.querySelector('svg title')?.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 70);
  if (text) return text;
  const hint = (el.className + ' ' + el.id + ' ' + (el.querySelector('svg')?.getAttribute('class') || '') + ' ' + (el.querySelector('[class*=icon]')?.className || '')).toString().match(/close|dismiss|cancel|back|menu|search|delete|trash|edit|plus|add|save|settings|more|check|copy|download|upload|chevron|arrow/i);
  return hint ? '(icon: ' + hint[0].toLowerCase() + ')' : '(icon button)';
};
const adDescribe = (ov) => ov ? (ov.getAttribute('aria-label') || ov.querySelector('h1,h2,h3,[class*=title]')?.innerText || ov.tagName.toLowerCase()).replace(/\s+/g, ' ').trim().slice(0, 60) : '';
"#;

/// Runs in the page: numbers the visible interactive elements and keeps them in `window.__adEls`.
/// When a dialog, drawer or menu is open, only what is inside it is listed: the page behind cannot be clicked.
const ELEMENTS_JS: &str = r#"(() => {
  __HELPERS__
  const sel = 'a[href],button,input,select,textarea,summary,[role=button],[role=link],[role=tab],[role=menuitem],[role=option],[role=checkbox],[role=switch],[onclick],[tabindex]:not([tabindex="-1"]),[contenteditable=true]';
  const ov = adOverlay();
  const scope = ov || document;
  window.__adEls = [];
  const out = [];
  const seen = new Set();
  for (const el of scope.querySelectorAll(sel)) {
    if (seen.has(el)) continue;
    seen.add(el);
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    if (r.width < 2 || r.height < 2 || cs.visibility === 'hidden' || cs.display === 'none' || +cs.opacity === 0 || el.disabled) continue;
    if (r.bottom < 0 || r.top > innerHeight || r.right < 0 || r.left > innerWidth) continue;
    const cx = Math.min(Math.max(r.left + r.width / 2, 1), innerWidth - 1), cy = Math.min(Math.max(r.top + r.height / 2, 1), innerHeight - 1);
    const top = document.elementFromPoint(cx, cy);
    if (top && top !== el && !el.contains(top) && !top.contains(el)) continue;
    const kind = el.tagName.toLowerCase() + (el.tagName === 'INPUT' ? '[' + (el.type || 'text') + ']' : '') + (el.getAttribute('role') ? '(' + el.getAttribute('role') + ')' : '');
    out.push('[' + window.__adEls.length + '] ' + kind + ' "' + adName(el) + '" at (' + Math.round(cx) + ',' + Math.round(cy) + ')');
    window.__adEls.push(el);
    if (out.length >= 80) break;
  }
  const doc = document.documentElement;
  const head = 'viewport ' + innerWidth + 'x' + innerHeight + ', scrolled ' + Math.round(scrollY) + ' of ' + Math.max(0, doc.scrollHeight - innerHeight) + ' px';
  const note = ov ? '\nOPEN OVERLAY: "' + adDescribe(ov) + '" (dialog, drawer or menu). The page behind it cannot be clicked. Only its own elements are listed below. To get back to the page, close it first: click its close/cancel button, or press Escape with browser_key, then call browser_elements again.' : '';
  return head + note + '\n' + (out.join('\n') || '(nothing to click in view)');
})()"#;

/// Runs in the page after a click: says whether a dialog, drawer or menu is open now.
const OVERLAY_NOTE_JS: &str = r#"(() => {
  __HELPERS__
  const ov = adOverlay();
  return ov ? 'A dialog, drawer or menu is open: "' + adDescribe(ov) + '". Close it (its close button, or Escape) before using the page behind it.' : 'No dialog, drawer or menu is open.';
})()"#;

/// Runs in the page with the target (`index`, `text` or `selector`): scrolls it into view and answers its centre.
fn locate_js(args: &Value) -> String {
    format!(
        r#"((arg) => {{
  let el = null;
  if (arg.index !== undefined && arg.index !== null) el = (window.__adEls || [])[arg.index];
  else if (arg.selector) el = document.querySelector(arg.selector);
  else if (arg.text) {{
    const want = String(arg.text).trim().toLowerCase();
    const label = (e) => (e.getAttribute('aria-label') || e.innerText || e.value || e.placeholder || e.title || '').replace(/\s+/g, ' ').trim().toLowerCase();
    const shown = (e) => {{ const r = e.getBoundingClientRect(); const cs = getComputedStyle(e); return r.width > 1 && r.height > 1 && cs.visibility !== 'hidden' && cs.display !== 'none'; }};
    const all = [...document.querySelectorAll('a,button,input,select,textarea,summary,label,li,h1,h2,h3,h4,p,span,div,[role]')].filter((e) => shown(e) && label(e).includes(want));
    all.sort((a, b) => (label(a) === want ? 0 : 1) - (label(b) === want ? 0 : 1) || label(a).length - label(b).length);
    el = all[0];
  }}
  if (!el || !el.isConnected) return {{ error: 'element not found: call browser_elements again and use a number from it' }};
  el.scrollIntoView({{ block: 'center', inline: 'center' }});
  const r = el.getBoundingClientRect();
  return {{ x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2), tag: el.tagName.toLowerCase(), label: (el.innerText || el.value || el.getAttribute('aria-label') || '').trim().slice(0, 60) }};
}})({})"#,
        args
    )
}

fn text_reply(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }] })
}

fn error_reply(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

/// The tools that are a few steps of page script and clicks, not one bridge route.
async fn element_tool(app: &AppHandle, name: &str, args: &Value, chat: &str) -> Value {
    let mut params = HashMap::new();
    if !chat.is_empty() {
        params.insert("chat".to_string(), chat.to_string());
    }
    // The page's answer is the script's value, possibly as text that holds JSON.
    let value_of = |answer: Value| match &answer["result"] {
        Value::String(text) => serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.clone())),
        other => other.clone(),
    };
    if name == "browser_elements" {
        return match crate::bridge::answer(app, "/eval", &params, &ELEMENTS_JS.replace("__HELPERS__", PAGE_HELPERS)).await {
            Ok(answer) => match value_of(answer) {
                Value::String(text) => text_reply(text),
                other => text_reply(other.to_string()),
            },
            Err(error) => error_reply(error),
        };
    }
    let target = match crate::bridge::answer(app, "/eval", &params, &locate_js(args)).await {
        Ok(answer) => value_of(answer),
        Err(error) => return error_reply(error),
    };
    if let Some(error) = target["error"].as_str() {
        return error_reply(error.to_string());
    }
    let (Some(x), Some(y)) = (target["x"].as_f64(), target["y"].as_f64()) else { return error_reply(format!("could not locate the element: {target}")) };
    params.insert("x".into(), x.to_string());
    params.insert("y".into(), y.to_string());
    match crate::bridge::answer(app, "/click", &params, "").await {
        Ok(_) => {
            // Let the page react (a drawer slides in, a menu opens), then say what is on top now.
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            let note = match crate::bridge::answer(app, "/eval", &params, &OVERLAY_NOTE_JS.replace("__HELPERS__", PAGE_HELPERS)).await {
                Ok(answer) => value_of(answer).as_str().unwrap_or_default().to_owned(),
                Err(_) => String::new(),
            };
            text_reply(format!("Clicked <{}> \"{}\" at ({x}, {y}). {note}", target["tag"].as_str().unwrap_or("element"), target["label"].as_str().unwrap_or_default()))
        }
        Err(error) => error_reply(error),
    }
}

fn tool_list() -> Value {
    Value::Array(
        TOOLS
            .iter()
            .map(|tool| {
                let properties: serde_json::Map<String, Value> = tool
                    .args
                    .iter()
                    .map(|(name, kind, about, _)| (name.to_string(), json!({ "type": kind, "description": about })))
                    .collect();
                let required: Vec<&str> = tool.args.iter().filter(|arg| arg.3).map(|arg| arg.0).collect();
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": { "type": "object", "properties": properties, "required": required },
                })
            })
            .chain(crate::index::tool_defs())
            .collect(),
    )
}

async fn call_tool(app: &AppHandle, name: &str, args: &Value, chat: &str) -> Value {
    if crate::index::is_tool(name) {
        return match crate::index::call(name, args, chat).await {
            Ok(text) => text_reply(text),
            Err(error) => error_reply(error),
        };
    }
    if matches!(name, "browser_elements" | "browser_click_element") {
        return element_tool(app, name, args, chat).await;
    }
    let Some(tool) = TOOLS.iter().find(|tool| tool.name == name) else {
        return json!({ "content": [{ "type": "text", "text": format!("unknown tool {name}") }], "isError": true });
    };
    let mut params = HashMap::new();
    if !chat.is_empty() {
        params.insert("chat".to_string(), chat.to_string());
    }
    let mut body = String::new();
    if let Some(map) = args.as_object() {
        for (key, value) in map {
            let text = match value {
                Value::String(text) => text.clone(),
                Value::Bool(true) => "1".into(),
                Value::Bool(false) => "0".into(),
                other => other.to_string(),
            };
            if key == "script" {
                body = text;
            } else {
                params.insert(key.clone(), text);
            }
        }
    }
    match crate::bridge::answer(app, tool.route, &params, &body).await {
        Ok(value) if tool.route == "/shot" => match value["png"].as_str().map(std::fs::read) {
            Some(Ok(bytes)) => {
                use base64::Engine;
                json!({ "content": [{ "type": "image", "mimeType": "image/png", "data": base64::engine::general_purpose::STANDARD.encode(bytes) }] })
            }
            _ => json!({ "content": [{ "type": "text", "text": value.to_string() }] }),
        },
        Ok(value) => {
            let text = match value {
                Value::String(text) => text,
                other => serde_json::to_string_pretty(&other).unwrap_or_default(),
            };
            json!({ "content": [{ "type": "text", "text": text }] })
        }
        Err(error) => json!({ "content": [{ "type": "text", "text": error }], "isError": true }),
    }
}

/// One JSON-RPC message. `None` for a notification (the HTTP answer is then 202 with no body).
pub async fn handle(app: &AppHandle, body: &str, chat: &str) -> Option<String> {
    let request: Value = serde_json::from_str(body).ok()?;
    let id = request.get("id").cloned()?;
    let params = &request["params"];
    let result = match request["method"].as_str().unwrap_or_default() {
        "initialize" => json!({
            "protocolVersion": params["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "agentdeck", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "The page open in AgentDeck's Preview panel, the browser the user is looking at. Use these tools to see it and act on it as a user would: screenshot, then click / type / scroll, then screenshot again and check browser_console and browser_network for errors. The code_* tools search an index of the project (symbols, outlines, definitions, references, imports): use them before grepping the tree.",
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tool_list() }),
        "tools/call" => call_tool(app, params["name"].as_str().unwrap_or_default(), &params["arguments"], chat).await,
        method => {
            return Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("unknown method {method}") } }).to_string());
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string())
}
