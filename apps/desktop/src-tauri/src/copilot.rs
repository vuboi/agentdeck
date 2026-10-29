//! GitHub Copilot SDK session runner and the small GitHub CLI credential bridge.
//!
//! The SDK owns the Copilot runtime and its JSON-RPC protocol. AgentDeck uses `gh` only to read or switch the active
//! GitHub account; the access token is passed to the SDK in memory and is never persisted by this module.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use github_copilot_sdk::handler::{PermissionHandler, PermissionResult};
use github_copilot_sdk::types::{
    Attachment, MessageOptions, PermissionRequestData, ResumeSessionConfig, SessionConfig,
};
use github_copilot_sdk::{Client, ClientOptions};
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
    request: TurnRequest,
    stop: ag_adapters::agent::interrupt::StopToken,
) {
    let result = run_turn_inner(&app, &chat_id, &turn_id, request, stop).await;
    forget_pending(&turn_id);
    if let Err(error) = result {
        emit(
            &app,
            &chat_id,
            &turn_id,
            AgentEvent::Error { message: error },
        );
    }
}

async fn run_turn_inner(
    app: &AppHandle,
    chat_id: &str,
    turn_id: &str,
    request: TurnRequest,
    mut stop: ag_adapters::agent::interrupt::StopToken,
) -> Result<(), String> {
    let token = github_token().await?;
    let client = start_client(token, request.cwd.as_deref()).await?;
    let session_id = request
        .session_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let permissions = Arc::new(CopilotPermissions {
        app: app.clone(),
        chat_id: chat_id.to_string(),
        turn_id: turn_id.to_string(),
        mode: Mode::parse(request.permission_mode.as_deref()),
        allowed: request.allowed_tools.clone(),
    });
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
        Err(error) => {
            let _ = client.stop().await;
            return Err(error.to_string());
        }
    };
    let mut events = session.subscribe();

    // Existing prompts name images and files by absolute path. Copilot's SDK accepts those same paths as native
    // attachments; keep the prompt intact so old messages and non-image files retain their current behavior.
    let attachments = prompt_attachments(&request.prompt);
    let message = if attachments.is_empty() {
        MessageOptions::new(request.prompt.clone())
    } else {
        MessageOptions::new(request.prompt.clone()).with_attachments(attachments)
    };
    let send = session.send_and_wait(message);
    tokio::pin!(send);
    let mut answer = String::new();
    let mut file_snapshots: HashMap<String, FileSnapshot> = HashMap::new();
    let mut children = Children::default();
    let mut finished = false;
    let mut turn_error = None;
    // After an abort the SDK still sends the end of the run (`session.idle`): read up to it so the stream is not cut mid-way.
    let mut aborting: Option<tokio::time::Instant> = None;
    loop {
        let abort_deadline = aborting.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(3600));
        tokio::select! {
            _ = stop.stopped(), if aborting.is_none() => {
                let _ = session.abort().await;
                aborting = Some(tokio::time::Instant::now() + ABORT_GRACE);
            }
            _ = tokio::time::sleep_until(abort_deadline), if aborting.is_some() => break,
            event = events.recv() => match event {
                Ok(event) => {
                    if aborting.is_some() && event.event_type == "session.idle" && event.agent_id.is_none() {
                        break;
                    }
                    // A sub-agent's events carry its `agentId`: they belong under the call that started it, not in the answer.
                    let task = children.lifecycle(&event.event_type, event.agent_id.as_deref(), &event.data);
                    let parent = children.parent_of(event.agent_id.as_deref(), &event.data);
                    if let Some(task) = task { emit(app, chat_id, turn_id, task); }
                    if event.event_type == "system.notification" {
                        // A shell that ran on its own finished: Copilot goes on by itself with a new turn.
                        let reason = event.data["kind"]["type"].as_str().unwrap_or("notification").to_string();
                        emit(app, chat_id, turn_id, AgentEvent::AutoTurn { reason });
                    }
                    if parent.is_none() {
                        if let Some(text) = event.data.get("deltaContent").and_then(Value::as_str) {
                            if event.event_type == "assistant.message_delta" { answer.push_str(text); }
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
                        "session.error" => Some(AgentEvent::Error { message: event.data["message"].as_str().unwrap_or("Copilot session failed").to_string() }),
                        _ => None,
                    };
                    if let Some(mapped) = mapped { emit(app, chat_id, turn_id, scoped(parent, mapped)); }
                }
                Err(_) => break,
            },
            result = &mut send => {
                // A stopped run ends here without an error: the stop is not a failure.
                if aborting.is_some() { break; }
                match result {
                    Ok(Some(event)) => {
                        // The returned message can be a sub-agent's: only the main agent's text is the answer.
                        let final_text = if event.agent_id.is_none() { event.data["content"].as_str().unwrap_or(&answer).to_string() } else { String::new() };
                        answer = if final_text.is_empty() { answer } else { final_text };
                        finished = true;
                    }
                    Ok(None) => finished = true,
                    Err(error) => turn_error = Some(error.to_string()),
                }
                break;
            }
        }
    }
    let _ = session.disconnect().await;
    let _ = client.stop().await;
    if let Some(error) = turn_error {
        return Err(error);
    }
    if finished {
        emit(
            app,
            chat_id,
            turn_id,
            AgentEvent::Done {
                session_id: Some(session_id),
                text: answer,
                cost_usd: None,
            },
        );
    }
    Ok(())
}

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

/// Starts a request-scoped SDK client using the account selected in GitHub CLI.
async fn start_client(token: String, cwd: Option<&Path>) -> Result<Client, String> {
    let mut options = ClientOptions::new()
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

fn emit(app: &AppHandle, session: &str, turn: &str, event: AgentEvent) {
    let _ = app.emit(
        "agent-event",
        AgentEnvelope {
            session: session.to_string(),
            turn: turn.to_string(),
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
