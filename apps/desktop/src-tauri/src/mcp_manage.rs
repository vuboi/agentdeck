//! The MCP tab: which MCP servers Claude Code, Codex and Antigravity know, whether each one answers, and adding or
//! removing one.
//!
//! Reading is read-only and never hands a secret to the UI: env values and header values stay in this process
//! (a test reads them again from the config file by id); the UI only gets the names of the env variables.
//! Changing goes through each CLI's own `mcp add` / `mcp remove`, so each tool keeps writing its own config.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::connectors::{capture, codex_home, command, failure, which, KillTree};

const AGENTS: [&str; 4] = ["claude", "codex", "antigravity", "copilot"];

/// One server as one config file has it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Raw {
    pub(crate) name: String,
    pub(crate) transport: String,
    pub(crate) command: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) url: Option<String>,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) agent: &'static str,
    pub(crate) scope: String,
    pub(crate) source: String,
    pub(crate) disabled: bool,
}

impl Raw {
    pub(crate) fn target(&self) -> String {
        match &self.url {
            Some(url) => url.clone(),
            None => std::iter::once(self.command.clone().unwrap_or_default())
                .chain(self.args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" "),
        }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct McpEntry {
    /// `<name>|<target>`: what `mcp_test` is given.
    id: String,
    name: String,
    /// `stdio`, `http` or `sse`.
    transport: String,
    /// The URL, or the command with its arguments.
    target: String,
    /// Names only: the values stay in the config file.
    env_keys: Vec<String>,
    agents: Vec<String>,
    enabled_agents: Vec<String>,
    scope: String,
    source: String,
    /// AgentDeck's own browser server.
    builtin: bool,
    disabled: bool,
}

#[derive(Serialize)]
pub struct McpOverview {
    /// Address of AgentDeck's browser MCP server, once it is listening.
    bridge: Option<String>,
    /// The URL Antigravity has stored for `agentdeck`, if it was linked.
    agy_link: Option<String>,
    /// Antigravity gets the AgentDeck browser by itself before each turn (the user has not turned that off).
    agy_auto: bool,
    entries: Vec<McpEntry>,
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn pairs(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string()),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `{ "mcpServers": { name: {...} } }` style maps (Claude's `.claude.json`, agy's `mcp_config.json`, Codex's TOML).
pub(crate) fn servers_in(map: &Value, agent: &'static str, scope: &str, source: &str) -> Vec<Raw> {
    let Some(object) = map.as_object() else {
        return Vec::new();
    };
    object
        .iter()
        .map(|(name, item)| {
            let url = item["url"]
                .as_str()
                .or_else(|| item["serverUrl"].as_str())
                .map(str::to_owned);
            let transport = match item["type"].as_str() {
                Some(kind) => kind.to_owned(),
                None if url.is_some() => "http".to_owned(),
                None => "stdio".to_owned(),
            };
            Raw {
                name: name.clone(),
                transport,
                command: item["command"].as_str().map(str::to_owned),
                args: strings(&item["args"]),
                env: pairs(&item["env"]),
                url,
                headers: pairs(item.get("headers").or_else(|| item.get("http_headers")).unwrap_or(&Value::Null)),
                agent,
                scope: scope.to_owned(),
                source: source.to_owned(),
                disabled: item["disabled"].as_bool().unwrap_or(false)
                    || item["enabled"].as_bool() == Some(false),
            }
        })
        .collect()
}

pub(crate) fn read_all() -> Vec<Raw> {
    let mut found = Vec::new();
    if let Ok(home) = crate::home_folder() {
        let claude = home.join(".claude.json");
        if let Ok(json) = std::fs::read_to_string(&claude)
            .map_err(drop)
            .and_then(|text| serde_json::from_str::<Value>(&text).map_err(drop))
        {
            let source = claude.to_string_lossy().into_owned();
            found.extend(servers_in(&json["mcpServers"], "claude", "user", &source));
            if let Some(projects) = json["projects"].as_object() {
                for (path, project) in projects {
                    found.extend(servers_in(
                        &project["mcpServers"],
                        "claude",
                        &format!("project: {path}"),
                        &source,
                    ));
                }
            }
        }
        let agy = home.join(".gemini").join("config").join("mcp_config.json");
        if let Ok(json) = std::fs::read_to_string(&agy)
            .map_err(drop)
            .and_then(|text| serde_json::from_str::<Value>(&text).map_err(drop))
        {
            found.extend(servers_in(
                &json["mcpServers"],
                "antigravity",
                "user",
                &agy.to_string_lossy(),
            ));
        }
        let copilot = home.join(".copilot").join("mcp-config.json");
        if let Ok(json) = std::fs::read_to_string(&copilot)
            .map_err(drop)
            .and_then(|text| serde_json::from_str::<Value>(&text).map_err(drop))
        {
            found.extend(servers_in(
                &json["mcpServers"],
                "copilot",
                "user",
                &copilot.to_string_lossy(),
            ));
        }
    }
    let codex = codex_home().join("config.toml");
    if let Ok(text) = std::fs::read_to_string(&codex) {
        if let Some(json) = toml::from_str::<toml::Value>(&text)
            .ok()
            .and_then(|value| serde_json::to_value(value).ok())
        {
            found.extend(servers_in(
                &json["mcp_servers"],
                "codex",
                "user",
                &codex.to_string_lossy(),
            ));
        }
    }
    if let Ok(home) = crate::home_folder() {
        for project in crate::store::sync::known_projects(&home) { found.extend(read_project(&project)); }
    }
    crate::mcp_providers::append_disabled(&mut found);
    found
}

fn bridge_url() -> Option<String> {
    std::env::var("AGENTDECK_BRIDGE")
        .ok()
        .map(|base| format!("{}/mcp", base.trim_end_matches('/')))
}

fn is_own(raw: &Raw) -> bool {
    raw.name == "agentdeck"
        && raw
            .url
            .as_deref()
            .is_some_and(|url| url.starts_with("http://127.0.0.1:") && url.ends_with("/mcp"))
}

/// The same server in several agents' configs becomes one row with all of them.
fn merge(raws: &[Raw]) -> Vec<McpEntry> {
    let mut entries: Vec<McpEntry> = Vec::new();
    for raw in raws.iter().filter(|raw| !is_own(raw)) {
        let target = raw.target();
        let id = raw_id(raw);
        match entries.iter_mut().find(|entry| entry.id == id) {
            Some(entry) => {
                if !raw.disabled && !entry.enabled_agents.contains(&raw.agent.to_owned()) { entry.enabled_agents.push(raw.agent.to_owned()); }
                entry.disabled = entry.disabled && raw.disabled;
                if !entry.agents.iter().any(|agent| agent == raw.agent) {
                    entry.agents.push(raw.agent.to_owned());
                }
                for key in raw.env.keys() {
                    if !entry.env_keys.contains(key) {
                        entry.env_keys.push(key.clone());
                    }
                }
            }
            None => entries.push(McpEntry {
                id,
                name: raw.name.clone(),
                transport: raw.transport.clone(),
                target,
                env_keys: raw.env.keys().cloned().collect(),
                agents: vec![raw.agent.to_owned()],
                enabled_agents: if raw.disabled { vec![] } else { vec![raw.agent.to_owned()] },
                scope: raw.scope.clone(),
                source: raw.source.clone(),
                builtin: false,
                disabled: raw.disabled,
            }),
        }
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

#[tauri::command]
pub async fn mcp_list() -> McpOverview {
    tauri::async_runtime::spawn_blocking(|| {
        let raws = read_all();
        let agy_link = raws
            .iter()
            .find(|raw| raw.agent == "antigravity" && is_own(raw))
            .and_then(|raw| raw.url.clone());
        let mut entries = merge(&raws);
        if let Some(url) = bridge_url() {
            // Claude, Codex and OpenCode get it with every turn; custom providers call it directly (free tiers never:
            // custom_chat.rs). Antigravity once it is linked.
            let mut agents = vec![
                "claude".to_owned(),
                "codex".to_owned(),
                "opencode".to_owned(),
                "copilot".to_owned(),
            ];
            if agy_link.is_some() {
                agents.push("antigravity".to_owned());
            }
            agents.push("custom".to_owned());
            entries.insert(
                0,
                McpEntry {
                    id: "agentdeck|builtin".to_owned(),
                    name: "agentdeck".to_owned(),
                    transport: "http".to_owned(),
                    target: url,
                    env_keys: Vec::new(),
                    enabled_agents: agents.clone(),
                    agents,
                    scope: "built in".to_owned(),
                    source: "AgentDeck browser".to_owned(),
                    builtin: true,
                    disabled: false,
                },
            );
        }
        let agy_auto = !agy_opt_out_file().is_some_and(|marker| marker.exists());
        McpOverview {
            bridge: bridge_url(),
            agy_link,
            agy_auto,
            entries,
        }
    })
    .await
    .unwrap_or(McpOverview {
        bridge: None,
        agy_link: None,
        agy_auto: true,
        entries: Vec::new(),
    })
}

// ---------------------------------------------------------------------------------------------------------------
// Test: initialize + tools/list

#[derive(Serialize, Debug, Default)]
pub struct TestResult {
    ok: bool,
    ms: u64,
    server: Option<String>,
    tools: Vec<String>,
    error: Option<String>,
}

pub(crate) fn initialize() -> Value {
    json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": { "name": "agentdeck", "version": env!("CARGO_PKG_VERSION") } } })
}

fn server_name(result: &Value) -> Option<String> {
    let info = &result["serverInfo"];
    let name = info["name"].as_str()?;
    Some(match info["version"].as_str() {
        Some(version) => format!("{name} {version}"),
        None => name.to_owned(),
    })
}

fn tool_names(result: &Value) -> Vec<String> {
    result["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The JSON-RPC message with `id` in a plain JSON body or in the `data:` lines of an event stream.
pub(crate) fn message_with_id(body: &str, id: i64) -> Option<Value> {
    let candidates = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .chain(std::iter::once(body.trim()));
    candidates
        .filter_map(|text| serde_json::from_str::<Value>(text).ok())
        .find(|message| message["id"].as_i64() == Some(id))
}

pub(crate) fn rpc_error(message: &Value) -> Option<String> {
    message.get("error").map(|error| {
        error["message"]
            .as_str()
            .map_or_else(|| error.to_string(), str::to_owned)
    })
}

async fn test_http(raw: &Raw) -> Result<(Option<String>, Vec<String>), String> {
    let url = raw.url.clone().ok_or("no address")?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let post = |body: &Value, session: Option<&str>| {
        let mut request = client
            .post(&url)
            .header("Accept", "application/json, text/event-stream")
            .json(body);
        for (key, value) in &raw.headers {
            request = request.header(key.as_str(), value.as_str());
        }
        if let Some(session) = session {
            request = request.header("Mcp-Session-Id", session);
        }
        request
    };
    let answer = |response: reqwest::Response| async move {
        if !response.status().is_success() {
            return Err(format!("the server answered {}", response.status()));
        }
        let session = response
            .headers()
            .get("mcp-session-id")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        Ok((
            session,
            response.text().await.map_err(|error| error.to_string())?,
        ))
    };
    let (session, body) = answer(
        post(&initialize(), None)
            .send()
            .await
            .map_err(|error| error.to_string())?,
    )
    .await?;
    let init = message_with_id(&body, 1).ok_or("no answer to initialize")?;
    if let Some(error) = rpc_error(&init) {
        return Err(error);
    }
    let _ = post(
        &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        session.as_deref(),
    )
    .send()
    .await;
    let (_, body) = answer(
        post(
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            session.as_deref(),
        )
        .send()
        .await
        .map_err(|error| error.to_string())?,
    )
    .await?;
    let listed = message_with_id(&body, 2).ok_or("no answer to tools/list")?;
    if let Some(error) = rpc_error(&listed) {
        return Err(error);
    }
    Ok((server_name(&init["result"]), tool_names(&listed["result"])))
}

async fn test_stdio(raw: &Raw) -> Result<(Option<String>, Vec<String>), String> {
    let name = raw.command.clone().ok_or("no command")?;
    let program = which(&name).unwrap_or_else(|| PathBuf::from(&name));
    let mut process = command(&program);
    process.args(&raw.args).stdin(Stdio::piped());
    for (key, value) in &raw.env {
        process.env(key, value);
    }
    let mut child = process
        .spawn()
        .map_err(|error| format!("could not start {name}: {error}"))?;
    // Declared after the child, so it is dropped first and takes the npx / shell / node tree with it.
    let _tree = KillTree::of(&child);
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let mut lines = BufReader::new(stdout).lines();

    async fn send(stdin: &mut tokio::process::ChildStdin, message: &Value) -> Result<(), String> {
        stdin
            .write_all(format!("{message}\n").as_bytes())
            .await
            .map_err(|error| error.to_string())?;
        stdin.flush().await.map_err(|error| error.to_string())
    }
    async fn reply(
        lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
        id: i64,
    ) -> Result<Value, String> {
        while let Some(line) = lines.next_line().await.map_err(|error| error.to_string())? {
            if let Some(message) = serde_json::from_str::<Value>(&line)
                .ok()
                .filter(|message| message["id"].as_i64() == Some(id))
            {
                return match rpc_error(&message) {
                    Some(error) => Err(error),
                    None => Ok(message["result"].clone()),
                };
            }
        }
        Err("the server closed before it answered".to_string())
    }

    send(&mut stdin, &initialize()).await?;
    let init = reply(&mut lines, 1).await?;
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await?;
    send(
        &mut stdin,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    )
    .await?;
    let listed = reply(&mut lines, 2).await?;
    Ok((server_name(&init), tool_names(&listed)))
}

/// Starts (stdio) or calls (http) a server the way an agent would, asks for its tools, and times it.
#[tauri::command]
pub async fn mcp_test(id: String) -> TestResult {
    let started = Instant::now();
    let found = if id == "agentdeck|builtin" {
        bridge_url().map(|url| Raw {
            name: "agentdeck".into(),
            transport: "http".into(),
            url: Some(url),
            agent: "claude",
            ..Raw::default()
        })
    } else {
        tauri::async_runtime::spawn_blocking(read_all)
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|raw| raw_id(raw) == id)
    };
    let outcome = match found {
        None => Err("this server is no longer in any config".to_string()),
        Some(raw) => {
            let work = async {
                if raw.url.is_some() {
                    test_http(&raw).await
                } else {
                    test_stdio(&raw).await
                }
            };
            // `npx` may have to download the server first.
            tokio::time::timeout(Duration::from_secs(40), work)
                .await
                .unwrap_or_else(|_| Err("no answer in 40 seconds".to_string()))
        }
    };
    let ms = started.elapsed().as_millis() as u64;
    match outcome {
        Ok((server, tools)) => TestResult {
            ok: true,
            ms,
            server,
            tools,
            error: None,
        },
        Err(error) => TestResult {
            ok: false,
            ms,
            server: None,
            tools: Vec::new(),
            error: Some(error),
        },
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Add and remove

#[derive(Deserialize, Debug)]
pub struct AddSpec {
    name: String,
    /// `stdio` or `http`.
    transport: String,
    /// The command (stdio) or the URL (http).
    target: String,
    #[serde(default)]
    args: Vec<String>,
    /// `KEY=value`.
    #[serde(default)]
    env: Vec<String>,
    /// `Name: value`.
    #[serde(default)]
    headers: Vec<String>,
    agents: Vec<String>,
}

#[derive(Serialize)]
pub struct AgentResult {
    agent: String,
    ok: bool,
    message: String,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

fn valid_env(pair: &str) -> bool {
    pair.split_once('=').is_some_and(|(key, _)| {
        !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn check(spec: &AddSpec) -> Result<(), String> {
    if !valid_name(&spec.name) {
        return Err("a name is letters, digits, - _ . (up to 64)".to_string());
    }
    if spec.agents.is_empty()
        || spec
            .agents
            .iter()
            .any(|agent| !AGENTS.contains(&agent.as_str()))
    {
        return Err("pick at least one agent".to_string());
    }
    match spec.transport.as_str() {
        "http" if spec.target.starts_with("http://") || spec.target.starts_with("https://") => {}
        "http" => return Err("the address must start with http:// or https://".to_string()),
        "stdio" if !spec.target.trim().is_empty() => {}
        "stdio" => return Err("give the command to run".to_string()),
        _ => return Err("the transport is stdio or http".to_string()),
    }
    if let Some(bad) = spec.env.iter().find(|pair| !valid_env(pair)) {
        return Err(format!("`{bad}` is not KEY=value"));
    }
    if spec.headers.iter().any(|header| !header.contains(':')) {
        return Err("a header is `Name: value`".to_string());
    }
    Ok(())
}

/// The arguments of `<agent> mcp add` for a spec. Pure, so each CLI's syntax is tested.
fn add_args(agent: &str, spec: &AddSpec) -> Result<Vec<String>, String> {
    let http = spec.transport == "http";
    let mut args: Vec<String> = vec!["mcp".into(), "add".into()];
    let push = |args: &mut Vec<String>, items: &[&str]| {
        args.extend(items.iter().map(|item| (*item).to_owned()))
    };
    match agent {
        "claude" => {
            push(&mut args, &["-s", "user"]);
            if http {
                push(
                    &mut args,
                    &["--transport", "http", &spec.name, &spec.target],
                );
                for header in &spec.headers {
                    push(&mut args, &["-H", header]);
                }
            } else {
                args.push(spec.name.clone());
                for pair in &spec.env {
                    push(&mut args, &["-e", pair]);
                }
                push(&mut args, &["--", &spec.target]);
                args.extend(spec.args.iter().cloned());
            }
        }
        "codex" => {
            args.push(spec.name.clone());
            if http {
                if !spec.headers.is_empty() {
                    return Err(
                        "Codex takes no custom headers (only a bearer-token variable)".to_string(),
                    );
                }
                push(&mut args, &["--url", &spec.target]);
            } else {
                for pair in &spec.env {
                    push(&mut args, &["--env", pair]);
                }
                push(&mut args, &["--", &spec.target]);
                args.extend(spec.args.iter().cloned());
            }
        }
        "antigravity" => {
            for pair in &spec.env {
                push(&mut args, &["-e", pair]);
            }
            for header in &spec.headers {
                push(&mut args, &["-H", header]);
            }
            args.push(spec.name.clone());
            if http {
                args.push(spec.target.clone());
            } else {
                push(&mut args, &["--", &spec.target]);
                args.extend(spec.args.iter().cloned());
            }
        }
        "copilot" => {
            return Err(
                "GitHub Copilot MCP settings are managed through the SDK session".to_string(),
            )
        }
        other => return Err(format!("unknown agent {other}")),
    }
    Ok(args)
}

fn binary_of(agent: &str) -> &'static str {
    match agent {
        "antigravity" => "agy",
        "codex" => "codex",
        _ => "claude",
    }
}

async fn run_cli(agent: &str, args: Vec<String>) -> AgentResult {
    let result = |ok: bool, message: String| AgentResult {
        agent: agent.to_owned(),
        ok,
        message,
    };
    let Some(program) = which(binary_of(agent)) else {
        return result(false, format!("{} is not installed", binary_of(agent)));
    };
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    match capture(&program, &refs, Duration::from_secs(40)).await {
        Ok(out) if out.ok => result(
            true,
            out.stdout
                .lines()
                .map(str::trim)
                .rfind(|line| !line.is_empty())
                .unwrap_or("done")
                .to_owned(),
        ),
        Ok(out) => result(false, failure(&out)),
        Err(error) => result(false, error),
    }
}

/// Adds a server to each chosen agent with that agent's own command.
#[tauri::command]
pub async fn mcp_add(spec: AddSpec) -> Result<Vec<AgentResult>, String> {
    check(&spec)?;
    let mut results = Vec::new();
    for agent in &spec.agents {
        if agent == "copilot" {
            let result = write_copilot_server(&spec);
            results.push(AgentResult {
                agent: agent.clone(),
                ok: result.is_ok(),
                message: result
                    .err()
                    .unwrap_or_else(|| "saved to Copilot MCP config".into()),
            });
            continue;
        }
        results.push(match add_args(agent, &spec) {
            Ok(args) => run_cli(agent, args).await,
            Err(message) => AgentResult {
                agent: agent.clone(),
                ok: false,
                message,
            },
        });
    }
    Ok(results)
}

/// Removes a server by name from the chosen agents.
#[tauri::command]
pub async fn mcp_remove(name: String, mut agents: Vec<String>, scope: Option<String>) -> Result<Vec<AgentResult>, String> {
    if !valid_name(&name) || name == "agentdeck" {
        return Err("that server cannot be removed here".to_string());
    }
    let removed = crate::mcp_providers::forget_disabled(&name, scope.as_deref().unwrap_or("user"), &agents)?;
    agents.retain(|a| !removed.contains(a));
    let removed_results = removed.into_iter().map(|agent| AgentResult { agent, ok: true, message: "Removed disabled assignment".into() }).collect::<Vec<_>>();
    if agents.is_empty() { return Ok(removed_results); }
    if let Some(project) = scope.as_deref().and_then(|s| s.strip_prefix("project: ")) {
        let project = PathBuf::from(project);
        if !crate::store::sync::known_projects(&crate::home_folder()?).contains(&project) { return Err("Unknown project scope".into()); }
        let mut results = removed_results;
        for agent in &agents {
            let file = match agent.as_str() { "claude" => ".mcp.json", "codex" => ".codex/config.toml", "copilot" => ".copilot/mcp-config.json", _ => continue };
            let result = crate::store::sync::remove_project_mcp(&project.join(file), &name, agent == "codex");
            results.push(AgentResult { agent: agent.clone(), ok: result.is_ok(), message: result.err().unwrap_or_else(|| "Removed from project".into()) });
        }
        return Ok(results);
    }
    let mut results = removed_results;
    for agent in agents
        .iter()
        .filter(|agent| AGENTS.contains(&agent.as_str()))
    {
        if agent == "copilot" {
            let result = remove_copilot_server(&name);
            results.push(AgentResult {
                agent: agent.clone(),
                ok: result.is_ok(),
                message: result
                    .err()
                    .unwrap_or_else(|| "removed from Copilot MCP config".into()),
            });
            continue;
        }
        let mut args: Vec<String> = vec!["mcp".into(), "remove".into()];
        if agent == "claude" {
            args.extend(["-s".to_string(), "user".to_string()]);
        }
        args.push(name.clone());
        results.push(run_cli(agent, args).await);
    }
    Ok(results)
}

/// Writes a Copilot SDK MCP server while retaining other servers in the user's config.
fn write_copilot_server(spec: &AddSpec) -> Result<(), String> {
    let path = crate::home_folder()?
        .join(".copilot")
        .join("mcp-config.json");
    let mut root = match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<Value>(&text)
            .map_err(|error| format!("Copilot MCP config is invalid: {error}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({"mcpServers": {}}),
        Err(error) => return Err(error.to_string()),
    };
    let Some(servers) = root["mcpServers"].as_object_mut() else {
        return Err("Copilot MCP config has an invalid mcpServers value".into());
    };
    let mut server = if spec.transport == "http" {
        json!({"type":"http", "url":spec.target, "headers": pairs_from_lines(&spec.headers)})
    } else {
        json!({"type":"stdio", "command":spec.target, "args":spec.args, "env":pairs_from_lines(&spec.env)})
    };
    if spec.transport == "stdio" && spec.args.is_empty() {
        server["args"] = json!([]);
    }
    servers.insert(spec.name.clone(), server);
    let parent = path.parent().ok_or("Copilot config has no parent folder")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let text = serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

/// Removes a server from the Copilot MCP config without affecting other entries.
fn remove_copilot_server(name: &str) -> Result<(), String> {
    let path = crate::home_folder()?
        .join(".copilot")
        .join("mcp-config.json");
    let mut root: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    let Some(servers) = root["mcpServers"].as_object_mut() else {
        return Err("Copilot MCP config has an invalid mcpServers value".into());
    };
    servers.remove(name);
    std::fs::write(
        path,
        serde_json::to_string_pretty(&root).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn pairs_from_lines(lines: &[String]) -> BTreeMap<String, String> {
    lines
        .iter()
        .filter_map(|line| {
            line.split_once('=')
                .or_else(|| line.split_once(':'))
                .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

/// Set when the user turned the AgentDeck browser off for Antigravity: it is then not linked again by itself.
fn agy_opt_out_file() -> Option<PathBuf> {
    Some(
        crate::home_folder()
            .ok()?
            .join(".agentdeck")
            .join("agy-browser-off"),
    )
}

/// Gives Antigravity the AgentDeck browser (`enable`) or takes it away. agy has no per-turn config flag, so this
/// is stored in its own config, and the address must be the current one (see `bridge::start`).
#[tauri::command]
pub async fn mcp_agy_link(enable: bool) -> Result<AgentResult, String> {
    let marker = agy_opt_out_file();
    let args: Vec<String> = if enable {
        let url = bridge_url().ok_or("the AgentDeck browser server is not running")?;
        if let Some(marker) = &marker {
            let _ = std::fs::remove_file(marker);
        }
        vec!["mcp".into(), "add".into(), "agentdeck".into(), url]
    } else {
        if let Some(marker) = &marker {
            if let Some(folder) = marker.parent() {
                let _ = std::fs::create_dir_all(folder);
            }
            let _ = std::fs::write(
                marker,
                "turned off in the MCP tab
",
            );
        }
        vec!["mcp".into(), "remove".into(), "agentdeck".into()]
    };
    Ok(run_cli("antigravity", args).await)
}

/// Before an Antigravity turn: makes sure it has the AgentDeck browser at the current address, so "debug the page"
/// works for it as it does for Claude and Codex. Skipped when the user turned it off, and when it is already right.
pub async fn ensure_agy_link() {
    if agy_opt_out_file().is_some_and(|marker| marker.exists()) {
        return;
    }
    let Some(url) = bridge_url() else { return };
    // Confirmed a moment ago for this address: the MCP and settings files are not read again on every turn.
    if let Ok(checked) = AGY_LINK_CHECKED.lock() {
        if checked
            .as_ref()
            .is_some_and(|(seen, at)| *seen == url && at.elapsed() < AGY_LINK_TTL)
        {
            return;
        }
    }
    let current = tauri::async_runtime::spawn_blocking(read_all)
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|raw| raw.agent == "antigravity" && is_own(raw))
        .and_then(|raw| raw.url);
    if current.as_deref() != Some(url.as_str()) {
        let added = run_cli(
            "antigravity",
            vec!["mcp".into(), "add".into(), "agentdeck".into(), url.clone()],
        )
        .await;
        // Not remembered as linked: the next turn tries again.
        if !added.ok {
            return;
        }
    }
    // A headless agy refuses any MCP tool that no rule allows ("a tool required the mcp permission that headless mode cannot prompt for"),
    // so linking alone leaves the tools unusable. This allows the AgentDeck browser's tools and nothing else.
    if let Ok(home) = crate::home_folder() {
        let settings = home
            .join(".gemini")
            .join("antigravity-cli")
            .join("settings.json");
        let _ = ag_adapters::agent::antigravity::add_allow_rules(
            &settings,
            &["mcp(agentdeck/*)".to_string()],
        );
    }
    if let Ok(mut checked) = AGY_LINK_CHECKED.lock() {
        *checked = Some((url, Instant::now()));
    }
}

/// The address the Antigravity link was last confirmed at, and when (see `ensure_agy_link`).
static AGY_LINK_CHECKED: std::sync::Mutex<Option<(String, Instant)>> = std::sync::Mutex::new(None);
const AGY_LINK_TTL: Duration = Duration::from_secs(60);

/// The MCP servers a custom provider may use in a chat: the ones in the user's Claude, Codex and Antigravity
/// configs (each name once), minus the disabled ones, AgentDeck's own browser, and other projects' servers.
pub(crate) fn servers_for(project: Option<&std::path::Path>) -> Vec<Raw> {
    servers_for_inner(project, None)
}

/// Native SDK providers must receive only their own explicitly enabled MCP assignments.
pub(crate) fn servers_for_agent(project: Option<&std::path::Path>, agent: &str) -> Vec<Raw> {
    servers_for_inner(project, Some(agent))
}

fn servers_for_inner(project: Option<&std::path::Path>, agent: Option<&str>) -> Vec<Raw> {
    let here = project.map(|dir| {
        dir.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    });
    let mut seen = std::collections::HashSet::new();
    let mut rows: Vec<_> = project.into_iter().flat_map(read_project).chain(read_all()).collect();
    rows.sort_by_key(|r| !r.scope.starts_with("project: "));
    rows.into_iter()
        .filter(|raw| !is_own(raw) && agent.is_none_or(|a| raw.agent == a))
        .filter(|raw| match raw.scope.strip_prefix("project: ") {
            Some(path) => {
                here.as_deref()
                    == Some(
                        path.replace('\\', "/")
                            .trim_end_matches('/')
                            .to_lowercase()
                            .as_str(),
                    )
            }
            None => true,
        })
        .filter(|raw| agent.is_some() || !raw.disabled)
        .filter(|raw| seen.insert(raw.name.clone()))
        .filter(|raw| !raw.disabled)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(transport: &str, target: &str) -> AddSpec {
        AddSpec {
            name: "files".into(),
            transport: transport.into(),
            target: target.into(),
            args: vec!["-y".into(), "server".into()],
            env: vec!["TOKEN=abc".into()],
            headers: Vec::new(),
            agents: vec!["claude".into()],
        }
    }

    #[test]
    fn servers_are_read_from_each_config_shape() {
        let claude = json!({ "a": { "type": "http", "url": "https://x.dev/mcp", "headers": { "Authorization": "Bearer secret" } },
                              "b": { "command": "npx", "args": ["-y", "m"], "env": { "KEY": "secret" } } });
        let raws = servers_in(&claude, "claude", "user", "f");
        assert_eq!(raws[0].transport, "http");
        assert_eq!(raws[1].transport, "stdio");
        assert_eq!(raws[1].target(), "npx -y m");
        let agy = servers_in(
            &json!({ "z": { "disabled": true, "serverUrl": "http://h/mcp" } }),
            "antigravity",
            "user",
            "f",
        );
        assert!(agy[0].disabled);
        assert_eq!(agy[0].url.as_deref(), Some("http://h/mcp"));
    }

    #[test]
    fn the_same_server_in_two_agents_is_one_row_without_secrets() {
        let one = servers_in(
            &json!({ "b": { "command": "npx", "args": ["m"], "env": { "KEY": "secret" } } }),
            "claude",
            "user",
            "f",
        );
        let two = servers_in(
            &json!({ "b": { "command": "npx", "args": ["m"] } }),
            "codex",
            "user",
            "g",
        );
        let merged = merge(&[one, two].concat());
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].agents, ["claude", "codex"]);
        assert_eq!(merged[0].env_keys, ["KEY"]);
        assert!(!format!("{merged:?}").contains("secret"));
    }

    #[test]
    fn answers_come_as_json_or_event_stream() {
        assert_eq!(
            message_with_id(
                r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"a"}]}}"#,
                2
            )
            .map(|m| tool_names(&m["result"])),
            Some(vec!["a".to_string()])
        );
        let stream = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"serverInfo\":{\"name\":\"s\",\"version\":\"1\"}}}\n\n";
        assert_eq!(
            message_with_id(stream, 1).and_then(|m| server_name(&m["result"])),
            Some("s 1".to_string())
        );
        assert_eq!(message_with_id(stream, 9), None);
    }

    #[test]
    fn each_cli_gets_its_own_syntax() {
        let stdio = spec("stdio", "npx");
        assert_eq!(
            add_args("claude", &stdio).unwrap_or_default(),
            [
                "mcp",
                "add",
                "-s",
                "user",
                "files",
                "-e",
                "TOKEN=abc",
                "--",
                "npx",
                "-y",
                "server"
            ]
        );
        assert_eq!(
            add_args("codex", &stdio).unwrap_or_default(),
            [
                "mcp",
                "add",
                "files",
                "--env",
                "TOKEN=abc",
                "--",
                "npx",
                "-y",
                "server"
            ]
        );
        assert_eq!(
            add_args("antigravity", &stdio).unwrap_or_default(),
            [
                "mcp",
                "add",
                "-e",
                "TOKEN=abc",
                "files",
                "--",
                "npx",
                "-y",
                "server"
            ]
        );
        let mut http = spec("http", "https://x.dev/mcp");
        http.headers = vec!["X-Key: 1".into()];
        assert_eq!(
            add_args("claude", &http).unwrap_or_default(),
            [
                "mcp",
                "add",
                "-s",
                "user",
                "--transport",
                "http",
                "files",
                "https://x.dev/mcp",
                "-H",
                "X-Key: 1"
            ]
        );
        assert!(add_args("codex", &http).is_err());
        assert_eq!(
            add_args("antigravity", &http).unwrap_or_default(),
            [
                "mcp",
                "add",
                "-e",
                "TOKEN=abc",
                "-H",
                "X-Key: 1",
                "files",
                "https://x.dev/mcp"
            ]
        );
    }

    #[test]
    fn bad_input_is_refused_before_any_command_runs() {
        assert!(check(&spec("stdio", "npx")).is_ok());
        let mut bad = spec("stdio", "npx");
        bad.name = "a b".into();
        assert!(check(&bad).is_err());
        bad = spec("http", "ftp://x");
        assert!(check(&bad).is_err());
        bad = spec("stdio", "npx");
        bad.env = vec!["nokey".into()];
        assert!(check(&bad).is_err());
        bad = spec("stdio", "npx");
        bad.agents = vec!["rm".into()];
        assert!(check(&bad).is_err());
    }
}

/// Reads project MCP config before global entries so a local override wins in SDK sessions.
fn read_project(project: &std::path::Path) -> Vec<Raw> {
    let mut found = Vec::new();
    for (file, agent, key, is_toml) in [(".mcp.json", "claude", "mcpServers", false), (".codex/config.toml", "codex", "mcp_servers", true), (".copilot/mcp-config.json", "copilot", "mcpServers", false)] {
        let path = project.join(file);
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let value: Option<Value> = if is_toml { toml::from_str::<toml::Value>(&text).ok().and_then(|v| serde_json::to_value(v).ok()) } else { serde_json::from_str(&text).ok() };
        if let Some(value) = value { found.extend(servers_in(&value[key], agent, &format!("project: {}", project.display()), &path.to_string_lossy())); }
    }
    found
}

/// Shares user MCP entries across providers or explicitly selected projects, preserving conflicts.
pub(crate) async fn sync_store(projects: &[PathBuf], name: Option<&str>) -> Vec<crate::store::sync::SyncResult> {
    use crate::store::sync::{merge_mcp, SyncResult};
    let all = read_all();
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for raw in all.iter().filter(|r| !is_own(r) && !r.disabled && (r.scope == "user" || projects.iter().any(|p| r.scope == format!("project: {}", p.display()))) && name.is_none_or(|n| n == r.name)) {
        if !seen.insert((raw.name.clone(), raw.scope.clone())) { continue; }
        if all.iter().any(|other| other.name == raw.name && other.scope == raw.scope && (other.command != raw.command || other.args != raw.args || other.url != raw.url || other.env != raw.env || other.headers != raw.headers || other.disabled)) {
            results.push(SyncResult { target: "global".into(), item: raw.name.clone(), ok: false, message: "Provider configurations differ; resolve the conflict before sync".into() });
            continue;
        }
        if projects.is_empty() {
            let agents = ["claude", "codex", "copilot"].into_iter().filter(|a| !all.iter().any(|r| r.name == raw.name && r.agent == *a && r.scope == "user")).map(str::to_owned).collect::<Vec<_>>();
            if agents.is_empty() { continue; }
            let spec = AddSpec { name: raw.name.clone(), transport: raw.transport.clone(), target: raw.url.clone().or_else(|| raw.command.clone()).unwrap_or_default(), args: raw.args.clone(), env: raw.env.iter().map(|(k,v)| format!("{k}={v}")).collect(), headers: raw.headers.iter().map(|(k,v)| format!("{k}: {v}")).collect(), agents };
            match mcp_add(spec).await {
                Ok(rows) => results.extend(rows.into_iter().map(|r| SyncResult { target: format!("global / {}", r.agent), item: raw.name.clone(), ok: r.ok, message: r.message })),
                Err(error) => results.push(SyncResult { target: "global".into(), item: raw.name.clone(), ok: false, message: error }),
            }
        } else {
            for project in projects.iter().filter(|p| raw.scope == "user" || raw.scope == format!("project: {}", p.display())) {
                for (file, key, is_toml) in [(".mcp.json", "mcpServers", false), (".codex/config.toml", "mcp_servers", true), (".copilot/mcp-config.json", "mcpServers", false)] {
                    let mut server = if let Some(url) = &raw.url { json!({"url":url}) } else { json!({"command":raw.command, "args":raw.args}) };
                    if !raw.env.is_empty() { server["env"] = json!(raw.env); }
                    if !raw.headers.is_empty() { server[if is_toml {"http_headers"} else {"headers"}] = json!(raw.headers); }
                    if !is_toml { server["type"] = json!(raw.transport); }
                    let result = if project.is_absolute() && project.is_dir() { merge_mcp(&project.join(file), key, &raw.name, server, is_toml) } else { Err("Project must be an existing absolute folder".into()) };
                    results.push(SyncResult { target: format!("{} / {file}", project.display()), item: raw.name.clone(), ok: result.is_ok(), message: result.err().unwrap_or_else(|| "Synchronized".into()) });
                }
            }
        }
    }
    results
}

pub(crate) fn raw_id(raw: &Raw) -> String {
    if raw.scope.starts_with("project: ") { format!("{}|{}|{}", raw.name, raw.target(), raw.scope) }
    else { format!("{}|{}", raw.name, raw.target()) }
}
