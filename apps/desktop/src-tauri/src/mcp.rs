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
    Tool { name: "browser_drag", route: "/drag", description: "Drags one element onto another, as HTML5 drag and drop (dragstart, dragover, drop). Both are CSS selectors.", args: &[("from", "string", "CSS selector of the element to drag", true), ("to", "string", "CSS selector of the element to drop on", true)] },
    Tool { name: "browser_upload", route: "/upload", description: "Uploads a file from disk: sets it on an <input type=file>, or drops it on a drop zone. Give the absolute file path.", args: &[("selector", "string", "CSS selector of the file input or drop zone", true), ("path", "string", "absolute path of the file", true)] },
    Tool { name: "browser_eval", route: "/eval", description: "Runs JavaScript in the page (or in a frame, see browser_frames) and returns its value. Use it to read state or find an element's position with getBoundingClientRect().", args: &[("script", "string", "an expression; promises are awaited", true), ("frame", "string", "frame index or part of its address", false)] },
    Tool { name: "browser_dom", route: "/dom", description: "HTML of the page, or of the elements matching a CSS selector.", args: &[("selector", "string", "", false), ("frame", "string", "", false)] },
    Tool { name: "browser_console", route: "/console", description: "Console lines and uncaught JavaScript errors of the page.", args: &[("level", "string", "error, warn or info", false), ("limit", "number", "", false)] },
    Tool { name: "browser_network", route: "/network", description: "Requests of the page; failed ones carry the start of the response body.", args: &[("failed", "boolean", "only failed requests", false), ("limit", "number", "", false)] },
    Tool { name: "browser_frames", route: "/frames", description: "Frames of other origins in the page (sign-in, payment, embeds).", args: &[] },
];

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
            .collect(),
    )
}

async fn call_tool(app: &AppHandle, name: &str, args: &Value) -> Value {
    let Some(tool) = TOOLS.iter().find(|tool| tool.name == name) else {
        return json!({ "content": [{ "type": "text", "text": format!("unknown tool {name}") }], "isError": true });
    };
    let mut params = HashMap::new();
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
pub async fn handle(app: &AppHandle, body: &str) -> Option<String> {
    let request: Value = serde_json::from_str(body).ok()?;
    let id = request.get("id").cloned()?;
    let params = &request["params"];
    let result = match request["method"].as_str().unwrap_or_default() {
        "initialize" => json!({
            "protocolVersion": params["protocolVersion"].as_str().unwrap_or("2025-06-18"),
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "agentdeck", "version": env!("CARGO_PKG_VERSION") },
            "instructions": "The page open in AgentDeck's Preview panel, the browser the user is looking at. Use these tools to see it and act on it as a user would: screenshot, then click / type / scroll, then screenshot again and check browser_console and browser_network for errors.",
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tool_list() }),
        "tools/call" => call_tool(app, params["name"].as_str().unwrap_or_default(), &params["arguments"]).await,
        method => {
            return Some(json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("unknown method {method}") } }).to_string());
        }
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string())
}
