use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde_json::{json, Value};

/// Runs AgentDeck CLI commands.
fn main() {
    if let Err(error) = run() {
        eprintln!("ag: {error:#}");
        std::process::exit(1);
    }
}

/// Dispatches supported CLI commands while preserving the existing version response.
fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("computer") => computer_command(args.collect()),
        _ => {
            println!("{}", version_output());
            Ok(())
        }
    }
}

/// Handles screen inspection and input through the running AgentDeck desktop bridge.
fn computer_command(args: Vec<String>) -> Result<()> {
    let Some(action) = args.first().map(String::as_str) else {
        print_computer_help();
        return Ok(());
    };
    if action == "--help" || action == "-h" || action == "help" {
        print_computer_help();
        return Ok(());
    }

    let (tool, arguments) = match action {
        "size" => ("computer_screen_size", json!({})),
        "screenshot" => ("computer_screenshot", json!({})),
        "click" => {
            let x = parse_i64(&args, 1, "x coordinate")?;
            let y = parse_i64(&args, 2, "y coordinate")?;
            ("computer_click", json!({ "x": x, "y": y }))
        }
        "type" => {
            let text = args.get(1..).unwrap_or_default().join(" ");
            if text.is_empty() {
                bail!("provide the text to type");
            }
            ("computer_type", json!({ "text": text }))
        }
        "key" => {
            let key = args.get(1).ok_or_else(|| anyhow!("provide a key name"))?;
            ("computer_press_key", json!({ "key": key }))
        }
        "scroll" => {
            let delta = parse_i64(&args, 1, "scroll delta")?;
            ("computer_scroll", json!({ "delta": delta }))
        }
        other => bail!("unknown computer action `{other}`; run `ag computer --help`"),
    };

    let result = call_computer_tool(tool, arguments)?;
    if action == "screenshot" {
        save_screenshot(&result, args.get(1).map(PathBuf::from).as_deref())
    } else {
        print_tool_text(&result);
        Ok(())
    }
}

/// Parses a required signed coordinate or scroll amount.
fn parse_i64(args: &[String], index: usize, label: &str) -> Result<i64> {
    args.get(index)
        .ok_or_else(|| anyhow!("provide the {label}"))?
        .parse::<i64>()
        .with_context(|| format!("{label} must be an integer"))
}

/// Prints the computer-use command syntax.
fn print_computer_help() {
    println!(
        "AgentDeck computer use\n\
         Usage: ag computer <action> [arguments]\n\
         Actions:\n\
           size                         Show primary display dimensions\n\
           screenshot [file.png]        Save a screenshot (defaults to a temporary PNG)\n\
           click <x> <y>                Click a point on the primary display\n\
           type <text>                  Type text into the focused app\n\
           key <name>                   Press a supported key (Enter, Tab, Escape, arrows, F1–F12…)\n\
           scroll <delta>               Scroll the wheel (positive up, negative down)\n\
         The AgentDeck desktop app must be running. Computer Use can be disabled in Settings."
    );
}

/// Calls one existing AgentDeck MCP tool using the authenticated loopback bridge.
fn call_computer_tool(name: &str, arguments: Value) -> Result<Value> {
    let endpoints = bridge_endpoints();
    if endpoints.is_empty() {
        bail!("AgentDeck is not running; start the desktop app and try again");
    }

    let mut connection_errors = Vec::new();
    for endpoint in endpoints {
        match mcp_tool_call(&endpoint.url, name, &arguments) {
            Ok(result) => return check_tool_result(result),
            Err(error) => connection_errors.push(format!("{}: {error}", endpoint.label)),
        }
    }
    bail!(
        "could not reach the AgentDeck desktop bridge ({})",
        connection_errors.join("; ")
    )
}

/// Finds the most recently started desktop bridge, preferring an explicit agent-process address.
fn bridge_endpoints() -> Vec<BridgeEndpoint> {
    let mut endpoints = Vec::new();
    if let Ok(url) = std::env::var("AGENTDECK_BRIDGE") {
        if !url.trim().is_empty() {
            endpoints.push(BridgeEndpoint {
                url,
                label: "AGENTDECK_BRIDGE".into(),
            });
        }
    }

    let Ok(app_data) = std::env::var("APPDATA") else {
        return endpoints;
    };
    let identifiers = [
        "dev.agentdeck.desktop.dev",
        "dev.agentdeck.desktop.local",
        "dev.agentdeck.desktop",
    ];
    let mut files: Vec<(PathBuf, Option<std::time::SystemTime>)> = identifiers
        .iter()
        .map(|identifier| Path::new(&app_data).join(identifier).join("bridge-url"))
        .filter_map(|path| {
            let modified = std::fs::metadata(&path).ok()?.modified().ok();
            Some((path, modified))
        })
        .collect();
    files.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
    for (path, _) in files {
        if let Ok(url) = std::fs::read_to_string(&path) {
            let url = url.trim().to_string();
            if !url.is_empty() && !endpoints.iter().any(|item| item.url == url) {
                endpoints.push(BridgeEndpoint {
                    url,
                    label: path.display().to_string(),
                });
            }
        }
    }
    endpoints
}

struct BridgeEndpoint {
    url: String,
    label: String,
}

/// Sends a JSON-RPC tools/call request only to a loopback address with AgentDeck's bridge token.
fn mcp_tool_call(endpoint: &str, name: &str, arguments: &Value) -> Result<Value> {
    let (address, target) = bridge_target(endpoint)?;
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": name, "arguments": arguments }
    });
    let body = serde_json::to_vec(&request)?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .with_context(|| "connection failed")?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    write!(
        stream,
        "POST {target}/mcp HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let (split, separator_len) = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|split| (split, 4))
        .or_else(|| {
            response
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|split| (split, 2))
        })
        .ok_or_else(|| {
            anyhow!(
                "bridge returned an incomplete HTTP response ({} bytes)",
                response.len()
            )
        })?;
    let head = std::str::from_utf8(&response[..split])?;
    if !head.lines().next().unwrap_or_default().contains(" 200 ") {
        bail!(
            "bridge rejected the request ({})",
            head.lines().next().unwrap_or("no status")
        );
    }
    let reply: Value = serde_json::from_slice(&response[split + separator_len..])
        .context("bridge returned invalid JSON")?;
    if let Some(error) = reply.get("error") {
        bail!(
            "MCP request failed: {}",
            error["message"].as_str().unwrap_or("unknown error")
        );
    }
    reply
        .get("result")
        .cloned()
        .ok_or_else(|| anyhow!("bridge response has no result"))
}

/// Validates the bridge URL and returns its loopback socket and tokenized path.
fn bridge_target(url: &str) -> Result<(SocketAddr, String)> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| anyhow!("bridge address must use local HTTP"))?;
    let (authority, token) = rest
        .split_once('/')
        .ok_or_else(|| anyhow!("bridge address has no token"))?;
    if authority.contains('@')
        || (authority.split(':').next() != Some("127.0.0.1")
            && authority.split(':').next() != Some("localhost"))
    {
        bail!("refusing a non-loopback AgentDeck bridge address");
    }
    let address = authority
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow!("bridge address did not resolve"))?;
    if !address.ip().is_loopback() {
        bail!("refusing a non-loopback AgentDeck bridge address");
    }
    let token = token.split('/').next().unwrap_or_default();
    if token.len() < 32 || !token.chars().all(|character| character.is_ascii_hexdigit()) {
        bail!("bridge token is invalid");
    }
    Ok((address, format!("/{token}")))
}

/// Converts an MCP tool error into a CLI error and returns a successful result.
fn check_tool_result(result: Value) -> Result<Value> {
    if result["isError"].as_bool().unwrap_or(false) {
        bail!(
            "{}",
            result["content"][0]["text"]
                .as_str()
                .unwrap_or("computer-use call failed")
        );
    }
    Ok(result)
}

/// Writes an MCP screenshot image to a requested file or a uniquely named temporary PNG.
fn save_screenshot(result: &Value, requested_path: Option<&Path>) -> Result<()> {
    let image = result["content"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["type"] == "image"))
        .ok_or_else(|| anyhow!("AgentDeck did not return a screenshot image"))?;
    let encoded = image["data"]
        .as_str()
        .ok_or_else(|| anyhow!("screenshot image has no data"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("screenshot data is invalid base64")?;
    let path = requested_path.map(Path::to_path_buf).unwrap_or_else(|| {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        std::env::temp_dir().join(format!("agentdeck-screenshot-{stamp}.png"))
    });
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("png"))
        != Some(true)
    {
        bail!("screenshot path must end in .png");
    }
    std::fs::write(&path, bytes).with_context(|| format!("could not write {}", path.display()))?;
    println!("Saved screenshot: {}", path.display());
    Ok(())
}

/// Prints the first textual result returned by an MCP computer-use tool.
fn print_tool_text(result: &Value) {
    if let Some(text) = result["content"]
        .as_array()
        .and_then(|items| items.iter().find_map(|item| item["text"].as_str()))
    {
        println!("{text}");
    } else {
        println!("{result}");
    }
}

fn version_output() -> String {
    format!("ag {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::version_output;

    #[test]
    fn version_output_includes_package_version() {
        assert_eq!(
            version_output(),
            format!("ag {}", env!("CARGO_PKG_VERSION"))
        );
    }
}
