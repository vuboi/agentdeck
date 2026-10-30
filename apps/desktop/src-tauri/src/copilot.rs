//! GitHub Copilot SDK session runner and the small GitHub CLI credential bridge.
//!
//! The SDK owns the Copilot runtime and its JSON-RPC protocol. AgentDeck uses `gh` only to read or switch the active
//! GitHub account; the access token is passed to the SDK in memory and is never persisted by this module.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use github_copilot_sdk::handler::{PermissionHandler, PermissionResult};
use github_copilot_sdk::types::{
    Attachment, DeliveryMode, MessageOptions, PermissionRequestData, ResumeSessionConfig, SessionConfig,
};
use github_copilot_sdk::{CliProgram, Client, ClientOptions};
use serde_json::{json, Value};
use sha2::Digest;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

use ag_adapters::agent::permission::{verdict, Action, Mode, Verdict};
use ag_adapters::agent::{
    begin_file_snapshot, file_change, finish_file_snapshot, AgentEvent, FileSnapshot, TaskStatus, TurnRequest,
};

use crate::connectors::{capture, which};

/// The event envelope sent to the existing chat timeline.
#[derive(Clone, serde::Serialize)]
struct AgentEnvelope {
    session: String,
    turn: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_id: Option<String>,
    event: AgentEvent,
}

/// Reads the active GitHub CLI token for the SDK without persisting or logging it.
pub(crate) async fn github_token() -> Result<String, String> {
    let gh = which("gh").ok_or("GitHub CLI (`gh`) is not installed")?;
    let output = capture(
        &gh,
        &["auth", "token", "--hostname", "github.com"],
        Duration::from_secs(10),
    )
    .await?;
    if !output.ok {
        return Err("Sign in to GitHub CLI first (`gh auth login`)".to_string());
    }
    let token = output.stdout.trim();
    if token.is_empty() {
        return Err("GitHub CLI returned an empty authentication token".to_string());
    }
    Ok(token.to_string())
}

/// Returns the Copilot models visible to the currently active `gh` account. The SDK starts its bundled Copilot CLI on first
/// use (it may be unpacked then), so a limit keeps the model list from waiting for ever with no message.
pub(crate) async fn models_list() -> Result<Vec<ag_adapters::models::ModelInfo>, String> {
    tokio::time::timeout(Duration::from_secs(60), list_models_now())
        .await
        .map_err(|_| "GitHub Copilot did not answer in time. Its CLI may still be starting: try again in a moment".to_string())?
}

/// The Copilot model catalogue, read from the same endpoint the Copilot clients use. The SDK's own `models.list` comes back
/// empty for this account (its CLI filters the list), so the endpoint is asked directly. Only the models the account may
/// use are kept (`policy.state` enabled). The token is sent to GitHub's Copilot host and nowhere else.
async fn list_models_now() -> Result<Vec<ag_adapters::models::ModelInfo>, String> {
    let token = github_token().await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get("https://api.githubcopilot.com/models")
        .bearer_auth(&token)
        .header("Copilot-Integration-Id", "vscode-chat")
        .header("Editor-Version", "vscode/1.99.0")
        .send()
        .await
        .map_err(|error| format!("GitHub Copilot could not be reached: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("GitHub Copilot refused the model list ({})", response.status()));
    }
    let body: Value = response.json().await.map_err(|error| error.to_string())?;
    let listed = body["data"]
        .as_array()
        .ok_or("GitHub Copilot sent a model list in an unexpected shape")?
        .iter()
        .filter(|model| model["policy"]["state"].as_str() == Some("enabled"))
        .filter_map(|model| {
            let value = model["id"].as_str()?.to_owned();
            let efforts = model["capabilities"]["supports"]["reasoning_effort"]
                .as_array()
                .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect())
                .unwrap_or_default();
            Some(ag_adapters::models::ModelInfo {
                label: model["name"].as_str().unwrap_or(&value).to_owned(),
                description: model["vendor"].as_str().unwrap_or_default().to_owned(),
                value,
                efforts,
                default_effort: None,
                hidden: false,
            })
        })
        .collect();
    Ok(listed)
}

/// Starts a Copilot SDK turn and translates its events to the chat UI's existing event contract.
pub(crate) async fn run_turn(
    app: AppHandle,
    chat_id: String,
    turn_id: String,
    message_id: Option<String>,
    request: TurnRequest,
    stop: ag_adapters::agent::interrupt::StopToken,
) {
    let permissions = Arc::new(CopilotPermissions {
        app: app.clone(),
        chat_id: chat_id.clone(),
        turn_id: turn_id.clone(),
        message_id: message_id.clone(),
        mode: Mode::parse(request.permission_mode.as_deref()),
        allowed: request.allowed_tools.clone(),
    });
    let sink = |event: AgentEvent| emit(&app, &chat_id, &turn_id, message_id.as_deref(), event);
    let result = run_turn_core(&chat_id, request, permissions, stop, sink).await;
    forget_pending(&turn_id);
    if let Err(error) = result {
        emit(&app, &chat_id, &turn_id, message_id.as_deref(), AgentEvent::Error { message: error });
    }
}

/// The turn without a window: starts the runtime, runs the message and sends every event to `out`. `chat_id` is where a
/// message written while the turn runs is taken from (`warm::inject`).
pub(crate) async fn run_turn_core(
    chat_id: &str,
    request: TurnRequest,
    permissions: Arc<dyn PermissionHandler>,
    stop: ag_adapters::agent::interrupt::StopToken,
    out: impl FnMut(AgentEvent),
) -> Result<(), String> {
    let token = github_token().await?;
    let (id, client) = acquire_client(chat_id, token, request.cwd.as_deref()).await?;
    let result = run_session(&client, chat_id, request, permissions, stop, out).await;
    if result.is_err() {
        // The CLI may be the cause (it did not start, or stopped answering): the next turn starts a new one.
        evict_client(id).await;
    }
    release_client(id).await;
    result
}

/// Runs one message on the shared client: a session for it, then its events until the run ends.
async fn run_session(
    client: &Client,
    chat_id: &str,
    request: TurnRequest,
    permissions: Arc<dyn PermissionHandler>,
    mut stop: ag_adapters::agent::interrupt::StopToken,
    mut out: impl FnMut(AgentEvent),
) -> Result<(), String> {
    let session_id = request
        .session_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mcp = mcp_servers(request.cwd.as_deref());
    let session_result = if request.resume {
        let mut config = ResumeSessionConfig::new(session_id.clone().into())
            .with_streaming(true)
            .with_permission_handler(permissions)
            .with_enable_session_store(true)
            .with_enable_skills(true);
        if let Some(model) = request.model.as_deref() {
            config = config.with_model(model);
        }
        if let Some(effort) = request.effort.as_deref() {
            config = config.with_reasoning_effort(effort);
        }
        if let Some(cwd) = request.cwd.as_deref() {
            config = config.with_working_directory(cwd);
        }
        if !mcp.is_empty() {
            config = config.with_mcp_servers(mcp);
        }
        let skills = copilot_skill_dirs();
        if !skills.is_empty() {
            config = config.with_skill_directories(skills);
        }
        client.resume_session(config).await
    } else {
        let mut config = SessionConfig::default()
            .with_session_id(session_id.clone())
            .with_streaming(true)
            .with_enable_session_store(true)
            .with_enable_skills(true)
            .with_permission_handler(permissions);
        if let Some(model) = request.model.as_deref() {
            config = config.with_model(model);
        }
        if let Some(effort) = request.effort.as_deref() {
            config = config.with_reasoning_effort(effort);
        }
        if let Some(cwd) = request.cwd.as_deref() {
            config = config.with_working_directory(cwd);
        }
        if !mcp.is_empty() {
            config = config.with_mcp_servers(mcp);
        }
        let skills = copilot_skill_dirs();
        if !skills.is_empty() {
            config = config.with_skill_directories(skills);
        }
        client.create_session(config).await
    };
    let session = match session_result {
        Ok(session) => session,
        Err(error) => return Err(error.to_string()),
    };
    let mut events = session.subscribe();
    // A message the user writes while the turn runs is steered into it (`immediate`): the work goes on and reads it at its
    // next model call, as a CLI that takes input at any time would.
    let (_inbox_slot, mut inbox) = ag_adapters::agent::warm::InjectSlot::open(chat_id);

    // Existing prompts name images and files by absolute path. Copilot's SDK accepts those same paths as native
    // attachments; keep the prompt intact so old messages and non-image files retain their current behavior.
    let attachments = prompt_attachments(&request.prompt);
    let message = if attachments.is_empty() {
        MessageOptions::new(request.prompt.clone())
    } else {
        MessageOptions::new(request.prompt.clone()).with_attachments(attachments)
    };
    // `send`, not `send_and_wait`: that one gives up after 60 s by default, a long turn is not a failure, and what comes
    // after the first idle (a shell that finishes, Copilot going on by itself) is read here from the events.
    if let Err(error) = session.send(message).await {
        let _ = session.disconnect().await;
        return Err(error.to_string());
    }
    let mut answer = String::new();
    let mut last_message: Option<String> = None;
    let mut file_snapshots: HashMap<String, FileSnapshot> = HashMap::new();
    let mut children = Children::default();
    let mut finished = false;
    let mut turn_error = None;
    // After an abort the SDK still sends the end of the run (`session.idle`): read up to it so the stream is not cut mid-way.
    let mut aborting: Option<tokio::time::Instant> = None;
    // The main agent went idle but the run did not end (a shell is still detached): how long to wait for what follows.
    let mut lingering: Option<tokio::time::Instant> = None;
    let far = || tokio::time::Instant::now() + Duration::from_secs(24 * 3600);
    loop {
        let abort_deadline = aborting.unwrap_or_else(far);
        let linger_deadline = lingering.unwrap_or_else(far);
        tokio::select! {
            _ = stop.stopped(), if aborting.is_none() => {
                let _ = session.abort().await;
                aborting = Some(tokio::time::Instant::now() + ABORT_GRACE);
            }
            _ = tokio::time::sleep_until(abort_deadline), if aborting.is_some() => break,
            _ = tokio::time::sleep_until(linger_deadline), if lingering.is_some() && aborting.is_none() => {
                // Nothing more came: the shell is still running. The turn ends with its answer; the shell dies with the runtime.
                finished = true;
                break;
            }
            Some(text) = inbox.recv(), if aborting.is_none() => {
                let mut steer = MessageOptions::new(text);
                steer.mode = Some(DeliveryMode::Immediate);
                if session.send(steer).await.is_ok() {
                    lingering = None;
                }
            }
            event = events.recv() => match event {
                Ok(event) => {
                    let root = event.agent_id.as_deref().is_none_or(str::is_empty);
                    if std::env::var_os("COPILOT_REAL_RAW").is_some() { eprintln!("RAW {} agent={:?} {}", event.event_type, event.agent_id, event.data.to_string().chars().take(160).collect::<String>()); } // DBG
                    if root && event.event_type == "session.idle" && std::env::var_os("COPILOT_REAL_LINGER").is_none() { // DBG
                        finished = aborting.is_none();
                        break;
                    }
                    if root && event.event_type == "assistant.idle" {
                        lingering = Some(tokio::time::Instant::now() + std::env::var("COPILOT_REAL_LINGER").ok().and_then(|v| v.parse().ok()).map_or(LINGER, Duration::from_secs)); // DBG
                    }
                    if root && matches!(event.event_type.as_str(), "assistant.turn_start" | "system.notification" | "tool.execution_start") {
                        lingering = None;
                    }
                    // A sub-agent's events carry its `agentId`: they belong under the call that started it, not in the answer.
                    let task = children.lifecycle(&event.event_type, event.agent_id.as_deref(), &event.data);
                    let parent = children.parent_of(event.agent_id.as_deref(), &event.data);
                    if let Some(task) = task { out(task); }
                    if event.event_type == "system.notification" {
                        // A shell that ran on its own finished: Copilot goes on by itself with a new turn.
                        let reason = event.data["kind"]["type"].as_str().unwrap_or("notification").to_string();
                        out(AgentEvent::AutoTurn { reason });
                    }
                    if parent.is_none() {
                        if event.event_type == "assistant.message_delta" {
                            if let Some(text) = event.data.get("deltaContent").and_then(Value::as_str) { answer.push_str(text); }
                        }
                        if event.event_type == "assistant.message" && root {
                            if let Some(text) = event.data["content"].as_str().filter(|text| !text.is_empty()) { last_message = Some(text.to_owned()); }
                        }
                    }
                    let mapped = match event.event_type.as_str() {
                        "assistant.message_delta" => event.data["deltaContent"].as_str().map(|text| AgentEvent::Delta { text: text.to_string() }),
                        "assistant.reasoning_delta" => event.data["deltaContent"].as_str().map(|text| AgentEvent::Thinking { text: text.to_string() }),
                        "tool.execution_start" => {
                            let id = event.data["toolCallId"].as_str().unwrap_or_default().to_string();
                            let name = event.data["toolName"].as_str().unwrap_or("tool").to_string();
                            let args = match event.data["arguments"].as_str() {
                                Some(raw) => serde_json::from_str::<Value>(raw).unwrap_or(Value::Null),
                                None => event.data["arguments"].clone(),
                            };
                            let change = file_change(&name, &args);
                            if change.is_none() {
                                if let Some(snapshot) = begin_file_snapshot(request.cwd.as_deref(), &name, &args) {
                                    file_snapshots.insert(id.clone(), snapshot);
                                }
                            }
                            let background = name == "task" || args["mode"] == "background";
                            Some(AgentEvent::ToolCall {
                                id,
                                name,
                                input: event.data["arguments"].as_str().map(str::to_string).unwrap_or_else(|| event.data["arguments"].to_string()),
                                change,
                                background,
                            })
                        }
                        "tool.execution_complete" => {
                            let id = event.data["toolCallId"].as_str().unwrap_or_default().to_string();
                            let is_error = !event.data["error"].is_null();
                            let snapshot = file_snapshots.remove(&id);
                            Some(AgentEvent::ToolResult {
                                id,
                                output: event.data["result"]["detailedContent"].as_str().or_else(|| event.data["result"]["content"].as_str()).unwrap_or_default().to_string(),
                                is_error,
                                image: None,
                                change: if is_error { None } else { snapshot.and_then(finish_file_snapshot) },
                            })
                        }
                        "assistant.usage" => Some(AgentEvent::Usage {
                            input_tokens: event.data["inputTokens"].as_u64().unwrap_or_default(),
                            output_tokens: event.data["outputTokens"].as_u64().unwrap_or_default(),
                            cache_read_tokens: event.data["cacheReadTokens"].as_u64().unwrap_or_default(),
                            cache_write_tokens: event.data["cacheWriteTokens"].as_u64().unwrap_or_default(),
                            new_call: true,
                        }),
                        "session.error" => {
                            turn_error = Some(event.data["message"].as_str().unwrap_or("Copilot session failed").to_string());
                            None
                        }
                        _ => None,
                    };
                    if let Some(mapped) = mapped { out(scoped(parent, mapped)); }
                    if turn_error.is_some() { break; }
                }
                Err(_) => break,
            },
        }
    }
    let _ = session.disconnect().await;
    if let Some(error) = turn_error {
        return Err(error);
    }
    if finished {
        out(AgentEvent::Done {
            session_id: Some(session_id),
            text: last_message.unwrap_or(answer),
            cost_usd: None,
        });
    }
    Ok(())
}

/// How long a Copilot CLI stays running with no turn. The same as the CLIs in `warm.rs`.
const CLIENT_IDLE: Duration = Duration::from_secs(20 * 60);

/// At most this many Copilot CLIs are kept, one per chat. The same limit as `warm.rs`.
const MAX_CLIENTS: usize = 4;

/// A Copilot CLI kept between the turns of one chat, for the token and working directory it was started with.
struct PooledClient {
    id: u64,
    chat: String,
    token: String,
    cwd: Option<PathBuf>,
    client: Client,
    /// Turns holding this client now.
    busy: usize,
    /// Changes at every turn start and end, so an idle timer stops the client only if nothing ran after it was set.
    generation: u64,
    last_used: tokio::time::Instant,
}

static POOL: LazyLock<tokio::sync::Mutex<Vec<PooledClient>>> = LazyLock::new(|| tokio::sync::Mutex::new(Vec::new()));
static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(1);

/// The pooled client of this chat. It is started when the chat has none, or when its account or folder changed; the old
/// one stops once no running turn still holds it. When the pool is full, the least recently used idle client makes room.
async fn acquire_client(chat: &str, token: String, cwd: Option<&Path>) -> Result<(u64, Client), String> {
    let cwd = cwd.map(Path::to_path_buf);
    let mut pool = POOL.lock().await;
    pool.retain(|entry| entry.chat != chat || (entry.token == token && entry.cwd == cwd));
    if !pool.iter().any(|entry| entry.chat == chat) {
        if pool.len() >= MAX_CLIENTS {
            let idle = pool.iter().enumerate().filter(|(_, entry)| entry.busy == 0).min_by_key(|(_, entry)| entry.last_used);
            if let Some((index, _)) = idle {
                pool.remove(index);
            }
        }
        let client = start_client(token.clone(), cwd.as_deref()).await?;
        let id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
        pool.push(PooledClient { id, chat: chat.to_owned(), token, cwd, client, busy: 0, generation: 0, last_used: tokio::time::Instant::now() });
    }
    let entry = pool.iter_mut().find(|entry| entry.chat == chat).ok_or("GitHub Copilot did not start")?;
    entry.busy += 1;
    entry.generation += 1;
    Ok((entry.id, entry.client.clone()))
}

/// Starts the chat's Copilot CLI ahead of its first message and keeps it in the pool. Errors are left to the first turn,
/// which reports them.
pub(crate) async fn prewarm(chat: String, cwd: Option<PathBuf>) {
    let Ok(token) = github_token().await else { return };
    if let Ok((id, _client)) = acquire_client(&chat, token, cwd.as_deref()).await {
        release_client(id).await;
    }
}

/// A turn is done with its client: the CLI stops once no turn has used it for `CLIENT_IDLE`.
async fn release_client(id: u64) {
    let generation = {
        let mut pool = POOL.lock().await;
        let Some(entry) = pool.iter_mut().find(|entry| entry.id == id) else {
            return;
        };
        entry.busy = entry.busy.saturating_sub(1);
        entry.generation += 1;
        entry.last_used = tokio::time::Instant::now();
        entry.generation
    };
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CLIENT_IDLE).await;
        let mut pool = POOL.lock().await;
        pool.retain(|entry| !(entry.id == id && entry.busy == 0 && entry.generation == generation));
    });
}

/// Drops a pooled client after a failed turn, so the next turn of that chat starts a new CLI.
async fn evict_client(id: u64) {
    POOL.lock().await.retain(|entry| entry.id != id);
}

/// How long the main agent may stay idle, with the run not over, before the turn ends with its answer: a shell it left
/// running is still going, and Copilot's own follow-up comes when that shell ends. Anything it does restarts the wait.
const LINGER: Duration = Duration::from_secs(15);

/// How long a stopped run is read for its end (`session.idle`) before it is given up.
const ABORT_GRACE: Duration = Duration::from_secs(5);

/// The sub-agents of a run: which tool call started each, so what they say and do is drawn under that call.
#[derive(Default)]
struct Children {
    parents: HashMap<String, String>,
}

impl Children {
    /// The tool call an event with this `agentId` belongs under; `None` for the main agent's own events.
    fn parent_of(&self, agent_id: Option<&str>, data: &Value) -> Option<String> {
        let agent = agent_id?;
        Some(
            self.parents
                .get(agent)
                .cloned()
                .or_else(|| data["parentToolCallId"].as_str().map(str::to_owned))
                .unwrap_or_else(|| agent.to_owned()),
        )
    }

    /// The task event a `subagent.*` event stands for, remembering which call started the sub-agent.
    fn lifecycle(&mut self, kind: &str, agent_id: Option<&str>, data: &Value) -> Option<AgentEvent> {
        if !matches!(kind, "subagent.started" | "subagent.completed" | "subagent.failed") {
            return None;
        }
        let tool = data["toolCallId"].as_str().map(str::to_owned);
        let id = agent_id.map(str::to_owned).or_else(|| tool.clone())?;
        let tools = data["totalToolCalls"].as_u64().and_then(|count| u32::try_from(count).ok());
        let name = data["agentDisplayName"].as_str().or_else(|| data["agentName"].as_str()).unwrap_or("sub-agent");
        let label = match data["agentDescription"].as_str().filter(|text| !text.is_empty()) {
            Some(description) => format!("{name}: {description}"),
            None => name.to_owned(),
        };
        match kind {
            "subagent.started" => {
                if let Some(tool) = &tool {
                    self.parents.insert(id.clone(), tool.clone());
                }
                Some(AgentEvent::Task { id, tool_use_id: tool, status: TaskStatus::Started, label: Some(label), summary: None, tools: None })
            }
            "subagent.completed" => {
                Some(AgentEvent::Task { id, tool_use_id: tool, status: TaskStatus::Done, label: Some(label), summary: None, tools })
            }
            _ => {
                let error = data["error"]["message"].as_str().or_else(|| data["error"].as_str()).map(str::to_owned);
                Some(AgentEvent::Task { id, tool_use_id: tool, status: TaskStatus::Failed, label: Some(label), summary: error, tools })
            }
        }
    }
}

/// An event of a sub-agent wrapped so the chat draws it under its parent call; the main agent's events and the ones
/// that are not drawn per block (usage, errors) pass unchanged.
fn scoped(parent: Option<String>, event: AgentEvent) -> AgentEvent {
    match (parent, &event) {
        (Some(parent), AgentEvent::Delta { .. } | AgentEvent::Thinking { .. } | AgentEvent::ToolCall { .. } | AgentEvent::ToolResult { .. }) => {
            AgentEvent::Scoped { parent, event: Box::new(event) }
        }
        _ => event,
    }
}

/// Starts a request-scoped SDK client using the account selected in GitHub CLI. The SDK runs the Copilot CLI that
/// AgentDeck installed (Settings > Connectors); it does not search PATH itself.
async fn start_client(token: String, cwd: Option<&Path>) -> Result<Client, String> {
    let cli = which("copilot").ok_or("GitHub Copilot CLI is not installed: install it in Settings > Connectors")?;
    let mut options = ClientOptions::new()
        .with_program(CliProgram::Path(cli))
        .with_github_token(token)
        .with_use_logged_in_user(false);
    if let Some(cwd) = cwd {
        options = options.with_cwd(cwd);
    }
    Client::start(options)
        .await
        .map_err(|error| error.to_string())
}

/// Converts AgentDeck's shared MCP configuration into the Copilot SDK's configuration format.
fn mcp_servers(
    cwd: Option<&Path>,
) -> indexmap::IndexMap<String, github_copilot_sdk::types::McpServerConfig> {
    crate::mcp_manage::servers_for_agent(cwd, "copilot")
        .into_iter()
        .filter(|server| server.agent == "claude" || server.agent == "codex" || server.agent == "antigravity" || server.agent == "copilot")
        .filter_map(|server| {
            let name = server.name.clone();
            let value = if let Some(url) = server.url {
                json!({ "type": "http", "url": url, "headers": server.headers })
            } else {
                json!({ "type": "stdio", "command": server.command?, "args": server.args, "env": server.env })
            };
            serde_json::from_value(value).ok().map(|config| (name, config))
        })
        .collect()
}

/// The shared Copilot skill folder populated by AgentDeck's Store assignment flow.
fn copilot_skill_dirs() -> Vec<PathBuf> {
    crate::home_folder()
        .ok()
        .map(|home| home.join(".copilot").join("skills"))
        .filter(|path| path.is_dir())
        .into_iter()
        .collect()
}

/// Reads the image and file paths from the existing composer attachment section.
fn prompt_attachments(prompt: &str) -> Vec<Attachment> {
    let mut attachments = Vec::new();
    let mut in_list = false;
    for line in prompt.lines() {
        if line.starts_with("Attached image") || line.starts_with("Attached file") {
            in_list = true;
            continue;
        }
        if in_list && line.starts_with("- ") {
            let candidate = line[2..].trim();
            let path = candidate
                .split_once(" (")
                .filter(|_| !Path::new(candidate).is_absolute() || !Path::new(candidate).exists())
                .map_or(candidate, |(path, _)| path);
            if Path::new(path).is_absolute() {
                attachments.push(Attachment::File {
                    path: PathBuf::from(path),
                    display_name: None,
                    line_range: None,
                });
            }
        } else if in_list && line.trim().is_empty() {
            in_list = false;
        }
    }
    attachments
}

/// A permission request the SDK is waiting on: the turn that asked, and where the card's answer goes.
type Waiting = (String, oneshot::Sender<bool>);

/// Permission requests the SDK is waiting on, by request id. The card's answer resolves them.
static PENDING: LazyLock<Mutex<HashMap<String, Waiting>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// The user's answer to a Copilot permission card. The SDK keeps the same turn going once its request is answered.
#[tauri::command]
pub(crate) fn copilot_permission_answer(request_id: String, allow: bool) -> Result<(), String> {
    let waiting = PENDING
        .lock()
        .map_err(|_| "permission state is unavailable".to_string())?
        .remove(&request_id);
    let (_, answer) = waiting.ok_or_else(|| "that permission request is no longer waiting".to_string())?;
    answer
        .send(allow)
        .map_err(|_| "the turn has already ended".to_string())
}

/// Drops the requests of a turn that ended; their waiters get a refusal.
fn forget_pending(turn_id: &str) {
    if let Ok(mut pending) = PENDING.lock() {
        pending.retain(|_, (turn, _)| turn != turn_id);
    }
}

/// Enforces AgentDeck's permission modes for Copilot SDK permission requests. The verdict of the mode comes from
/// `permission::verdict`, so Copilot reads the five modes the same way the other agents do.
struct CopilotPermissions {
    app: AppHandle,
    chat_id: String,
    turn_id: String,
    message_id: Option<String>,
    mode: Mode,
    allowed: Vec<String>,
}

/// What a Copilot permission kind does, as the modes classify actions.
fn action_of(kind: &str) -> Action {
    match kind {
        "read" => Action::Read,
        "write" => Action::Write,
        "shell" => Action::Shell,
        _ => Action::Other,
    }
}

#[async_trait::async_trait]
impl PermissionHandler for CopilotPermissions {
    async fn handle(
        &self,
        _session_id: github_copilot_sdk::types::SessionId,
        request_id: github_copilot_sdk::types::RequestId,
        data: PermissionRequestData,
    ) -> PermissionResult {
        let kind = data
            .kind
            .map(|kind| format!("{kind:?}").to_lowercase())
            .unwrap_or_else(|| "tool".into());
        let tool = data.extra["toolName"].as_str().unwrap_or(&kind).to_string();
        let summary = ["fullCommandText", "fileName", "path", "url", "toolTitle"]
            .iter()
            .find_map(|key| data.extra.get(*key).and_then(Value::as_str))
            .unwrap_or("Copilot requested permission to use a tool")
            .to_string();
        // The guard only makes a call stricter and runs before the mode and before any earlier "always allow": a heavy command is
        // refused in every mode, a risky one is asked about each time (plan mode still refuses every command).
        let mut careful = None;
        if kind == "shell" {
            let command = data.extra.get("fullCommandText").and_then(Value::as_str).unwrap_or(&summary);
            match ag_adapters::agent::guard::assess(command) {
                Some(ag_adapters::agent::guard::Risk::Block(reason)) => return PermissionResult::reject(Some(reason.to_string())),
                Some(ag_adapters::agent::guard::Risk::Confirm(reason)) if self.mode != Mode::Plan => careful = Some(reason),
                _ => {}
            }
        }
        let summary = match careful {
            Some(reason) => format!("⚠ {reason} {summary}"),
            None => summary,
        };
        let permission_key = format!("{kind}\0{tool}\0{summary}");
        let digest = sha2::Sha256::digest(permission_key.as_bytes());
        let digest = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let allow = format!("copilot:{kind}:{digest}");
        if careful.is_none() && self.allowed.contains(&allow) {
            return PermissionResult::approve_once();
        }
        match if careful.is_some() { Verdict::Ask } else { verdict(self.mode, action_of(&kind)) } {
            Verdict::Approve => return PermissionResult::approve_once(),
            Verdict::Refuse => {
                return PermissionResult::reject(Some(
                    "Plan mode is read-only. Describe the change instead of making it.".to_string(),
                ));
            }
            Verdict::Ask => {}
        }
        let (answer, waiting) = oneshot::channel();
        let id = request_id.to_string();
        if let Ok(mut pending) = PENDING.lock() {
            pending.insert(id.clone(), (self.turn_id.clone(), answer));
        }
        emit(
            &self.app,
            &self.chat_id,
            &self.turn_id,
            self.message_id.as_deref(),
            AgentEvent::PermissionDenied {
                requests: vec![ag_adapters::agent::PermissionRequest {
                    id,
                    tool,
                    summary,
                    allow,
                }],
            },
        );
        match waiting.await {
            Ok(true) => PermissionResult::approve_once(),
            _ => PermissionResult::reject(Some("The user did not allow this tool.".to_string())),
        }
    }
}

pub(crate) fn emit(app: &AppHandle, session: &str, turn: &str, message_id: Option<&str>, event: AgentEvent) {
    // Only the main window: a Preview page is a copy of this app and must not replay the chats' events.
    let _ = app.emit_to(
        "main",
        "agent-event",
        AgentEnvelope {
            session: session.to_string(),
            turn: turn.to_string(),
            message_id: message_id.map(str::to_owned),
            event,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sub_agent_is_a_task_and_its_events_go_under_the_call_that_started_it() {
        let mut children = Children::default();
        let started = json!({ "toolCallId": "call_1", "agentName": "general-purpose", "agentDisplayName": "subdone-agent", "agentDescription": "Return completion marker" });
        let task = children.lifecycle("subagent.started", Some("agent-1"), &started);
        assert_eq!(
            task,
            Some(AgentEvent::Task {
                id: "agent-1".into(),
                tool_use_id: Some("call_1".into()),
                status: TaskStatus::Started,
                label: Some("subdone-agent: Return completion marker".into()),
                summary: None,
                tools: None,
            })
        );
        assert_eq!(children.parent_of(Some("agent-1"), &json!({})), Some("call_1".into()));
        assert_eq!(children.parent_of(None, &json!({})), None, "the main agent's events stay in the answer");
        let delta = scoped(children.parent_of(Some("agent-1"), &json!({})), AgentEvent::Delta { text: "SUBDONE".into() });
        assert!(matches!(delta, AgentEvent::Scoped { ref parent, .. } if parent == "call_1"));
        let usage = AgentEvent::Usage { input_tokens: 1, output_tokens: 1, cache_read_tokens: 0, cache_write_tokens: 0, new_call: true };
        assert_eq!(scoped(Some("call_1".into()), usage.clone()), usage, "usage is counted on the message, not under a card");
        let done = children.lifecycle("subagent.completed", Some("agent-1"), &json!({ "toolCallId": "call_1", "totalToolCalls": 3 }));
        assert!(matches!(done, Some(AgentEvent::Task { status: TaskStatus::Done, tools: Some(3), .. })));
        assert!(children.lifecycle("assistant.message", Some("agent-1"), &json!({})).is_none());
    }

    #[test]
    fn an_unknown_sub_agent_falls_back_to_the_parent_named_in_the_event() {
        let children = Children::default();
        assert_eq!(children.parent_of(Some("a"), &json!({ "parentToolCallId": "call_9" })), Some("call_9".into()));
        assert_eq!(children.parent_of(Some("a"), &json!({})), Some("a".into()));
    }

    #[test]
    fn a_failed_sub_agent_carries_its_error() {
        let mut children = Children::default();
        let failed = children.lifecycle("subagent.failed", Some("a"), &json!({ "toolCallId": "c", "error": "boom" }));
        assert!(matches!(failed, Some(AgentEvent::Task { status: TaskStatus::Failed, summary: Some(ref text), .. }) if text == "boom"));
    }
}

/// Runs the turn against the REAL Copilot runtime with the account `gh` has active. Off by default (network, quota):
///
/// ```text
/// cargo test -p agentdeck-desktop --lib copilot_real -- --ignored --nocapture
/// ```
///
/// `COPILOT_REAL_MODEL` picks the model (default `gpt-5.6-luna`, low effort) and `COPILOT_REAL_ONLY` one scenario by
/// name (`plain`, `tools`, `subagent`, `background`, `steer`, `abort`). Every event is printed with its time.
#[cfg(test)]
mod real {
    use super::*;
    use ag_adapters::agent::interrupt::StopHandle;
    use github_copilot_sdk::types::{PermissionRequestData, RequestId, SessionId};

    struct ApproveAll;

    #[async_trait::async_trait]
    impl PermissionHandler for ApproveAll {
        async fn handle(&self, _session: SessionId, _request: RequestId, _data: PermissionRequestData) -> PermissionResult {
            PermissionResult::approve_once()
        }
    }

    type Log = Arc<Mutex<Vec<(u128, AgentEvent)>>>;

    struct Run {
        events: Vec<(u128, AgentEvent)>,
        result: Result<(), String>,
        took: Duration,
    }

    impl Run {
        fn kinds(&self) -> Vec<&AgentEvent> {
            self.events.iter().map(|(_, event)| event).collect()
        }
        fn done_text(&self) -> Option<&str> {
            self.events.iter().rev().find_map(|(_, event)| match event {
                AgentEvent::Done { text, .. } => Some(text.as_str()),
                _ => None,
            })
        }
        fn main_text(&self) -> String {
            self.events
                .iter()
                .filter_map(|(_, event)| match event {
                    AgentEvent::Delta { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        }
    }

    fn short(event: &AgentEvent) -> String {
        let text = format!("{event:?}");
        if text.len() > 230 { format!("{}…", text.chars().take(230).collect::<String>()) } else { text }
    }

    /// One scenario: the prompt, an optional second message sent after a delay (as the chat's inject does), an optional stop.
    async fn scenario(name: &str, prompt: &str, steer: Option<(u64, &str)>, stop_after: Option<u64>, limit: u64) -> Run {
        let dir = std::env::temp_dir().join(format!("agentdeck-copilot-real-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("work folder");
        let chat = format!("copilot-real-{name}-{}", std::process::id());
        let request = TurnRequest {
            prompt: prompt.to_owned(),
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            resume: false,
            model: Some(std::env::var("COPILOT_REAL_MODEL").unwrap_or_else(|_| "gpt-5.6-luna".to_owned())),
            permission_mode: None,
            effort: Some("low".to_owned()),
            allowed_tools: Vec::new(),
            cwd: Some(dir),
            owner: Some(chat.clone()),
            api_key: None,
        };
        let log: Log = Arc::default();
        let started = std::time::Instant::now();
        let (handle, token) = StopHandle::new();
        let sink_log = Arc::clone(&log);
        let sink = move |event: AgentEvent| {
            let at = started.elapsed().as_millis();
            println!("[{name} {at:>6} ms] {}", short(&event));
            sink_log.lock().expect("log").push((at, event));
        };
        let side_chat = chat.clone();
        let steer = steer.map(|(after, text)| (after, text.to_owned()));
        let helper = tokio::spawn(async move {
            if let Some((after, text)) = steer {
                tokio::time::sleep(Duration::from_secs(after)).await;
                let taken = ag_adapters::agent::warm::inject(&side_chat, &text);
                println!("[steer] inject taken = {taken}");
            }
            if let Some(after) = stop_after {
                tokio::time::sleep(Duration::from_secs(after)).await;
                println!("[stop] asked");
                handle.stop();
            }
            // Keeps the handle alive as long as the helper waits for a stop that never comes.
            tokio::time::sleep(Duration::from_secs(3600)).await;
        });
        let result = tokio::time::timeout(
            Duration::from_secs(limit),
            run_turn_core(&chat, request, Arc::new(ApproveAll), token, sink),
        )
        .await
        .unwrap_or_else(|_| Err(format!("the turn did not end in {limit} s")));
        helper.abort();
        let events = log.lock().expect("log").clone();
        println!("[{name}] ended after {:?}: {result:?}", started.elapsed());
        Run { events, result, took: started.elapsed() }
    }

    fn wanted(name: &str) -> bool {
        std::env::var("COPILOT_REAL_ONLY").map_or(true, |only| only == name)
    }

    #[tokio::test]
    #[ignore = "uses the real Copilot account (network, quota): see the module docs"]
    async fn copilot_real() {
        if wanted("plain") {
            let run = scenario("plain", "Reply with exactly the word PONG and nothing else.", None, None, 120).await;
            assert!(run.result.is_ok(), "{:?}", run.result);
            assert!(run.done_text().is_some_and(|text| text.contains("PONG")), "{:?}", run.done_text());
            assert!(matches!(run.kinds().last(), Some(AgentEvent::Done { .. })), "Done is the last event");
            assert!(!run.kinds().iter().any(|event| matches!(event, AgentEvent::Scoped { .. })));
        }
        if wanted("tools") {
            let run = scenario(
                "tools",
                "Using your tools, create a file named hello.txt containing the word HELLO, then run the shell command `echo done`, then reply with the word FINISHED.",
                None,
                None,
                240,
            )
            .await;
            assert!(run.result.is_ok(), "{:?}", run.result);
            let calls = run.kinds().iter().filter(|event| matches!(event, AgentEvent::ToolCall { .. })).count();
            let results = run.kinds().iter().filter(|event| matches!(event, AgentEvent::ToolResult { .. })).count();
            assert!(calls >= 2 && results >= 2, "calls={calls} results={results}");
            assert!(matches!(run.kinds().last(), Some(AgentEvent::Done { .. })));
        }
        if wanted("subagent") {
            let run = scenario(
                "subagent",
                "Use the task tool to start one general-purpose sub-agent that replies with only the word SUBDONE. Then reply with exactly the word RELAYED.",
                None,
                None,
                240,
            )
            .await;
            assert!(run.result.is_ok(), "{:?}", run.result);
            let started = run.kinds().iter().any(|event| matches!(event, AgentEvent::Task { status: TaskStatus::Started, .. }));
            let done = run.kinds().iter().any(|event| matches!(event, AgentEvent::Task { status: TaskStatus::Done, .. }));
            println!("[subagent] task started={started} done={done}");
            let calls: Vec<String> = run
                .kinds()
                .iter()
                .filter_map(|event| match event {
                    AgentEvent::ToolCall { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .collect();
            for event in run.kinds() {
                if let AgentEvent::Scoped { parent, .. } = event {
                    assert!(calls.contains(parent), "a scoped event names a call that was shown: {parent}");
                }
            }
            assert!(!run.main_text().contains("SUBDONE"), "the sub-agent's words stay out of the main answer: {}", run.main_text());
            assert!(matches!(run.kinds().last(), Some(AgentEvent::Done { .. })));
        }
        if wanted("background") {
            let run = scenario(
                "background",
                "Start this PowerShell command as a DETACHED background shell (async mode, do not wait for it): `Start-Sleep -Seconds 8; Write-Output BGDONE`. Reply STARTED right away. When the shell finishes, report its output.",
                None,
                None,
                240,
            )
            .await;
            assert!(run.result.is_ok(), "{:?}", run.result);
            let auto = run.kinds().iter().filter(|event| matches!(event, AgentEvent::AutoTurn { .. })).count();
            println!("[background] auto turns = {auto}, took {:?}, answer = {:?}", run.took, run.done_text());
            assert!(matches!(run.kinds().last(), Some(AgentEvent::Done { .. })), "the turn ends, it never hangs");
            assert!(run.took < Duration::from_secs(200));
        }
        if wanted("steer") {
            let run = scenario(
                "steer",
                "Run the shell command `Start-Sleep -Seconds 12` and wait for it, then reply with exactly FIRST-DONE.",
                Some((5, "Also include the word SECOND-MSG in your final answer.")),
                None,
                240,
            )
            .await;
            assert!(run.result.is_ok(), "{:?}", run.result);
            let text = run.done_text().unwrap_or_default().to_owned();
            println!("[steer] answer = {text:?}");
            assert!(matches!(run.kinds().last(), Some(AgentEvent::Done { .. })));
            assert!(text.contains("SECOND-MSG"), "the steered message was read: {text}");
        }
        if wanted("abort") {
            let run = scenario(
                "abort",
                "Run the shell command `Start-Sleep -Seconds 60` and wait for it, then reply DONE.",
                None,
                Some(6),
                120,
            )
            .await;
            assert!(run.result.is_ok(), "a stop is not a failure: {:?}", run.result);
            assert!(!run.kinds().iter().any(|event| matches!(event, AgentEvent::Error { .. })));
            assert!(run.took < Duration::from_secs(30), "the stop ended the turn: {:?}", run.took);
        }
    }
}
