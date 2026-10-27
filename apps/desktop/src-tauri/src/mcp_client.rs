//! A small MCP client, so a custom provider can use the servers the user set up for Claude, Codex or Antigravity.
//!
//! Both ways a server is reached are covered: a command spoken to over stdin and stdout, and an address that takes
//! JSON-RPC over HTTP (the answer is JSON or an event stream). A server is started when a turn starts, asked for its
//! tools once, called while the turn runs, and stopped with the turn (its process is killed when this is dropped).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

use crate::connectors::{command, which, KillTree};
use crate::mcp_manage::{initialize, message_with_id, rpc_error, Raw};

#[allow(clippy::large_enum_variant)] // one per server for one turn: boxing buys nothing
enum Link {
    Http {
        client: reqwest::Client,
        url: String,
        headers: BTreeMap<String, String>,
        session: Option<String>,
    },
    Stdio {
        // Before the child: dropped first, so the whole tree (npx, the shell, node) goes with it.
        _tree: KillTree,
        _child: Child,
        stdin: ChildStdin,
        lines: tokio::io::Lines<BufReader<ChildStdout>>,
    },
}

/// What one tool call gave back.
pub struct Reply {
    pub text: String,
    pub is_error: bool,
    pub image: Option<String>,
}

pub struct Server {
    pub name: String,
    /// The tools as the server lists them: `name`, `description`, `inputSchema`.
    pub tools: Vec<Value>,
    link: Link,
    next: i64,
}

impl Link {
    async fn send(&mut self, message: &Value) -> Result<Option<String>, String> {
        match self {
            Link::Http {
                client,
                url,
                headers,
                session,
            } => {
                let mut request = client
                    .post(url.as_str())
                    .header("Accept", "application/json, text/event-stream")
                    .json(message);
                for (key, value) in headers.iter() {
                    request = request.header(key.as_str(), value.as_str());
                }
                if let Some(id) = session.as_deref() {
                    request = request.header("Mcp-Session-Id", id);
                }
                let response = request.send().await.map_err(|error| error.to_string())?;
                if !response.status().is_success() {
                    return Err(format!("the server answered {}", response.status()));
                }
                if let Some(id) = response
                    .headers()
                    .get("mcp-session-id")
                    .and_then(|value| value.to_str().ok())
                {
                    *session = Some(id.to_owned());
                }
                response
                    .text()
                    .await
                    .map(Some)
                    .map_err(|error| error.to_string())
            }
            Link::Stdio { stdin, .. } => {
                stdin
                    .write_all(format!("{message}\n").as_bytes())
                    .await
                    .map_err(|error| error.to_string())?;
                stdin.flush().await.map_err(|error| error.to_string())?;
                Ok(None)
            }
        }
    }

    /// Sends a request and waits for the answer with the same id; `Err` carries the server's own error text.
    async fn request(&mut self, id: i64, method: &str, params: Value) -> Result<Value, String> {
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let body = self.send(&message).await?;
        let answer = match (&mut *self, body) {
            (_, Some(body)) => message_with_id(&body, id).ok_or("the server gave no answer")?,
            (Link::Stdio { lines, .. }, None) => loop {
                let line = lines
                    .next_line()
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or("the server closed before it answered")?;
                if let Some(found) = serde_json::from_str::<Value>(&line)
                    .ok()
                    .filter(|found| found["id"].as_i64() == Some(id))
                {
                    break found;
                }
            },
            _ => return Err("the server gave no answer".into()),
        };
        match rpc_error(&answer) {
            Some(error) => Err(error),
            None => Ok(answer["result"].clone()),
        }
    }
}

impl Server {
    /// Starts or reaches the server and reads its tools.
    pub async fn start(raw: &Raw) -> Result<Server, String> {
        let link = if let Some(url) = &raw.url {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .map_err(|error| error.to_string())?;
            Link::Http {
                client,
                url: url.clone(),
                headers: raw.headers.clone(),
                session: None,
            }
        } else {
            let name = raw.command.clone().ok_or("no command")?;
            let program = which(&name).unwrap_or_else(|| PathBuf::from(&name));
            let mut process = command(&program);
            process
                .args(&raw.args)
                .stdin(Stdio::piped())
                .stderr(Stdio::null());
            for (key, value) in &raw.env {
                process.env(key, value);
            }
            let mut child = process
                .spawn()
                .map_err(|error| format!("could not start {name}: {error}"))?;
            let stdin = child.stdin.take().ok_or("no stdin")?;
            let stdout = child.stdout.take().ok_or("no stdout")?;
            Link::Stdio {
                _tree: KillTree::of(&child),
                _child: child,
                stdin,
                lines: BufReader::new(stdout).lines(),
            }
        };
        let mut server = Server {
            name: raw.name.clone(),
            tools: Vec::new(),
            link,
            next: 1,
        };
        let init = initialize();
        server
            .link
            .request(1, "initialize", init["params"].clone())
            .await?;
        let _ = server
            .link
            .send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await;
        server.next = 2;
        let listed = server.link.request(2, "tools/list", json!({})).await?;
        server.tools = listed["tools"].as_array().cloned().unwrap_or_default();
        server.next = 3;
        Ok(server)
    }

    pub async fn call(&mut self, tool: &str, args: &Value) -> Reply {
        self.next += 1;
        let id = self.next;
        let work = self
            .link
            .request(id, "tools/call", json!({ "name": tool, "arguments": args }));
        let result = match tokio::time::timeout(Duration::from_secs(120), work).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                return Reply {
                    text: error,
                    is_error: true,
                    image: None,
                }
            }
            Err(_) => {
                return Reply {
                    text: "the tool did not answer in 120 seconds".into(),
                    is_error: true,
                    image: None,
                }
            }
        };
        let mut text = Vec::new();
        let mut image = None;
        for part in result["content"].as_array().into_iter().flatten() {
            match part["type"].as_str() {
                Some("text") => text.push(part["text"].as_str().unwrap_or_default().to_owned()),
                Some("image") => {
                    image = Some(format!(
                        "data:{};base64,{}",
                        part["mimeType"].as_str().unwrap_or("image/png"),
                        part["data"].as_str().unwrap_or_default()
                    ))
                }
                _ => {}
            }
        }
        if text.is_empty() {
            if let Some(structured) = result.get("structuredContent") {
                text.push(structured.to_string());
            }
        }
        Reply {
            text: text.join("\n"),
            is_error: result["isError"].as_bool().unwrap_or(false),
            image,
        }
    }
}
