//! What the Preview agent learned about the pages of a site, kept in the webview's IndexedDB (src/lib/page-memory.ts) so the
//! next visit can look an element up instead of taking a screenshot and walking the DOM again.
//!
//! The webview owns the store: each call is emitted as a `page-memory` event `{ callId, op, args }` and, for the ones that
//! answer, waits until the webview replies through `page_memory_reply`. Elements seen by `browser_elements` and
//! `browser_find` are stored on the way (fire and forget); `browser_recall` reads, `browser_remember` keeps a recipe.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

const WAIT: Duration = Duration::from_secs(8);

pub const TOOLS: [&str; 2] = ["browser_recall", "browser_remember"];

pub fn is_tool(name: &str) -> bool {
    TOOLS.contains(&name)
}

pub fn tool_defs() -> Vec<Value> {
    vec![
        json!({
            "name": "browser_recall",
            "description": "Looks up, in AgentDeck's memory of this site, the elements you (or an earlier run) already found on its pages: buttons, links, fields, each with the CSS selector that found it and whether it is on the page you are on now. Use it BEFORE taking a screenshot or listing the page: when it answers, click or fill with browser_click_element / browser_fill using the selector and skip the scan. It also returns steps that worked before for a similar goal. Say what you want in a few words (\"sign in\", \"export report\", \"search box\"). An empty answer means nothing is remembered yet: use browser_elements or browser_find, and the result is remembered for next time.",
            "inputSchema": { "type": "object", "properties": {
                "query": { "type": "string", "description": "what you are looking for or want to do, in a few words" },
                "limit": { "type": "number", "description": "most elements to return (default 8)" }
            }, "required": ["query"] },
        }),
        json!({
            "name": "browser_remember",
            "description": "Keeps the steps that worked to reach a goal on this site, so the next run can replay them with browser_recall instead of working it out again. Call it once the goal was reached, with the goal in a few words and the steps in order, one per line, each naming the element and the action (for example: click \"Sign in\" (#login) / fill the email field (input[name=email]) / click \"Continue\").",
            "inputSchema": { "type": "object", "properties": {
                "goal": { "type": "string", "description": "what the steps achieve, in a few words" },
                "steps": { "type": "string", "description": "the steps, one per line" }
            }, "required": ["goal", "steps"] },
        }),
    ]
}

static NEXT: AtomicU64 = AtomicU64::new(1);
type Answer = Result<String, String>;
static PENDING: LazyLock<Mutex<HashMap<String, oneshot::Sender<Answer>>>> =
    LazyLock::new(Default::default);

/// Hands a read or a write to the webview and waits for its answer.
pub async fn ask(app: &AppHandle, op: &str, args: Value) -> Result<String, String> {
    let id = format!("pm-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let (sender, receiver) = oneshot::channel();
    if let Ok(mut pending) = PENDING.lock() {
        pending.insert(id.clone(), sender);
    }
    if let Err(error) = app.emit("page-memory", json!({ "callId": id, "op": op, "args": args })) {
        if let Ok(mut pending) = PENDING.lock() {
            pending.remove(&id);
        }
        return Err(error.to_string());
    }
    match tokio::time::timeout(WAIT, receiver).await {
        Ok(Ok(answer)) => answer,
        _ => {
            if let Ok(mut pending) = PENDING.lock() {
                pending.remove(&id);
            }
            Err("the page memory did not answer".to_string())
        }
    }
}

/// A write nobody waits for: what was seen on a page.
pub fn store(app: &AppHandle, args: Value) {
    let _ = app.emit("page-memory", json!({ "callId": "", "op": "store", "args": args }));
}

/// The webview's answer to a call.
#[tauri::command]
pub fn page_memory_reply(call_id: String, text: String, is_error: bool) -> bool {
    let sender = PENDING
        .lock()
        .ok()
        .and_then(|mut pending| pending.remove(&call_id));
    sender.is_some_and(|sender| sender.send(if is_error { Err(text) } else { Ok(text) }).is_ok())
}

/// Runs in the page: the selectors of the elements the agent was shown (`window.__adEls`), steadiest form first, with
/// what they are called. A selector that matches more than one element is left out.
pub const MEMO_JS: &str = r#"(() => {
  const els = window.__adEls || [];
  const uniq = (q) => { try { return document.querySelectorAll(q).length === 1; } catch (e) { return false; } };
  const esc = (s) => (window.CSS && CSS.escape ? CSS.escape(s) : s);
  const steady = (id) => id && !/\d{4,}|^[:_]|[0-9a-f]{8,}/i.test(id);
  const selector = (el) => {
    const tag = el.tagName.toLowerCase();
    if (steady(el.id) && uniq('#' + esc(el.id))) return '#' + esc(el.id);
    for (const attr of ['data-testid', 'data-test', 'data-cy', 'data-qa', 'aria-label', 'name', 'placeholder', 'title', 'alt']) {
      const v = el.getAttribute(attr);
      if (v && v.length < 80) { const q = tag + '[' + attr + '=' + JSON.stringify(v) + ']'; if (uniq(q)) return q; }
    }
    const parts = [];
    for (let cur = el; cur && cur !== document.body && cur.parentElement && parts.length < 5; cur = cur.parentElement) {
      let part = cur.tagName.toLowerCase();
      if (steady(cur.id)) { parts.unshift('#' + esc(cur.id)); if (uniq(parts.join(' > '))) return parts.join(' > '); break; }
      const same = [...cur.parentElement.children].filter((c) => c.tagName === cur.tagName);
      if (same.length > 1) part += ':nth-of-type(' + (same.indexOf(cur) + 1) + ')';
      parts.unshift(part);
      if (uniq(parts.join(' > '))) return parts.join(' > ');
    }
    return '';
  };
  const name = (el) => (el.getAttribute('aria-label') || el.innerText || el.value || el.placeholder || el.title || el.alt || el.name || '').replace(/\s+/g, ' ').trim().slice(0, 80);
  const kind = (el) => el.tagName.toLowerCase() + (el.tagName === 'INPUT' ? '[' + (el.type || 'text') + ']' : '') + (el.getAttribute('role') ? '(' + el.getAttribute('role') + ')' : '');
  const out = [];
  for (const el of els) {
    if (!el || !el.isConnected) continue;
    const s = selector(el);
    if (s) out.push({ selector: s, name: name(el), kind: kind(el) });
    if (out.length >= 120) break;
  }
  return JSON.stringify({ url: location.href, elements: out });
})()"#;

/// Runs in the page: whether each remembered selector is on the page now, and its text.
pub fn verify_js(selectors: &[String]) -> String {
    format!(
        r#"((list) => JSON.stringify(list.map((s) => {{
  let el = null; try {{ el = document.querySelector(s); }} catch (e) {{}}
  if (!el) return {{ present: false }};
  const r = el.getBoundingClientRect(); const cs = getComputedStyle(el);
  return {{ present: true, visible: r.width > 1 && r.height > 1 && cs.visibility !== 'hidden' && cs.display !== 'none', text: (el.getAttribute('aria-label') || el.innerText || el.value || el.placeholder || '').replace(/\s+/g, ' ').trim().slice(0, 60) }};
}})))({})"#,
        serde_json::to_string(selectors).unwrap_or_else(|_| "[]".into())
    )
}

/// The reply of `browser_recall`: what the memory found, checked against the page now.
pub fn format_recall(found: &Value, live: &[Value]) -> String {
    let hits = found["hits"].as_array().cloned().unwrap_or_default();
    let recipes = found["recipes"].as_array().cloned().unwrap_or_default();
    if hits.is_empty() && recipes.is_empty() {
        return format!(
            "Nothing remembered for \"{}\" on {}. Use browser_elements or browser_find: what they show is remembered for next time.",
            found["query"].as_str().unwrap_or_default(),
            found["site"].as_str().unwrap_or("this site")
        );
    }
    let mut lines = vec![format!("Remembered on {}:", found["site"].as_str().unwrap_or("this site"))];
    for (index, hit) in hits.iter().enumerate() {
        let state = match live.get(index) {
            Some(check) if check["present"] == true && check["visible"] == true => "on this page now".to_string(),
            Some(check) if check["present"] == true => "on this page, hidden".to_string(),
            Some(_) => format!("not on this page (seen on {})", hit["pathKey"].as_str().unwrap_or("/")),
            None => String::new(),
        };
        lines.push(format!(
            "- {} \"{}\" {} · seen {}x · {}",
            hit["kind"].as_str().unwrap_or("element"),
            hit["name"].as_str().unwrap_or_default(),
            hit["selector"].as_str().unwrap_or_default(),
            hit["seen"].as_u64().unwrap_or(1),
            state
        ));
    }
    if !hits.is_empty() {
        lines.push("Use the selector with browser_click_element or browser_fill. One marked \"not on this page\" needs its page first (browser_open); one that fails there changed: use browser_elements.".to_string());
    }
    for recipe in recipes {
        lines.push(format!("Steps that worked for \"{}\":", recipe["goal"].as_str().unwrap_or_default()));
        for (number, step) in recipe["steps"].as_array().into_iter().flatten().enumerate() {
            lines.push(format!("  {}. {}", number + 1, step.as_str().unwrap_or_default()));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_memory_says_what_to_do_next() {
        let text = format_recall(&json!({ "site": "shop.com", "query": "sign in", "hits": [], "recipes": [] }), &[]);
        assert!(text.contains("Nothing remembered") && text.contains("browser_elements"));
    }

    #[test]
    fn a_hit_is_marked_with_what_the_page_shows_now() {
        let found = json!({ "site": "shop.com", "hits": [
            { "selector": "#login", "name": "Sign in", "kind": "button", "seen": 3, "pathKey": "/login" },
            { "selector": "#gone", "name": "Old", "kind": "a", "seen": 1, "pathKey": "/old" } ],
            "recipes": [ { "goal": "sign in", "steps": ["click Sign in", "fill email"] } ] });
        let live = [json!({ "present": true, "visible": true }), json!({ "present": false })];
        let text = format_recall(&found, &live);
        assert!(text.contains("button \"Sign in\" #login · seen 3x · on this page now"));
        assert!(text.contains("not on this page (seen on /old)"));
        assert!(text.contains("1. click Sign in"));
    }

    #[test]
    fn the_tools_are_named_and_described() {
        let defs = tool_defs();
        assert_eq!(defs.len(), TOOLS.len());
        assert!(TOOLS.iter().all(|name| defs.iter().any(|tool| tool["name"] == *name)));
    }
}
