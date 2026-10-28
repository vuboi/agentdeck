//! GitHub Copilot SDK session runner and the small GitHub CLI credential bridge.
//!
//! The SDK owns the Copilot runtime and its JSON-RPC protocol. AgentDeck uses `gh` only to read or switch the active
//! GitHub account; the access token is passed to the SDK in memory and is never persisted by this module.

use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use github_copilot_sdk::handler::{PermissionHandler, PermissionResult};
use github_copilot_sdk::types::{
    Attachment, MessageOptions, PermissionRequestData, ResumeSessionConfig, SessionConfig,
};
use github_copilot_sdk::{Client, ClientOptions};
use serde_json::{json, Value};
use sha2::Digest;
use tauri::{AppHandle, Emitter};

use ag_adapters::agent::{
    begin_file_snapshot, file_change, finish_file_snapshot, AgentEvent, FileSnapshot, TurnRequest,
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

/// Returns the Copilot models visible to the currently active `gh` account.
pub(crate) async fn models_list() -> Result<Vec<ag_adapters::models::ModelInfo>, String> {
    let token = github_token().await?;
    let client = start_client(token, None).await?;
    let models = client.list_models().await;
    let stopped = client.stop().await;
    let models = models.map_err(|error| error.to_string())?;
    stopped.map_err(|error| error.to_string())?;
    let listed = models
        .into_iter()
        .map(|model| ag_adapters::models::ModelInfo {
            value: model.id,
            label: model.name,
            description: model.vendor.unwrap_or_default(),
            efforts: model.supported_reasoning_efforts.unwrap_or_default(),
            default_effort: model.default_reasoning_effort,
            hidden: false,
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
    let mut finished = false;
    let mut turn_error = None;
    loop {
        tokio::select! {
            _ = stop.stopped() => {
                let _ = session.abort().await;
                break;
            }
            event = events.recv() => match event {
                Ok(event) => {
                    if let Some(text) = event.data.get("deltaContent").and_then(Value::as_str) {
                        if event.event_type == "assistant.message_delta" { answer.push_str(text); }
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
                            Some(AgentEvent::ToolCall {
                                id,
                                name,
                                input: event.data["arguments"].as_str().map(str::to_string).unwrap_or_else(|| event.data["arguments"].to_string()),
                                change,
                                background: false,
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
                    if let Some(mapped) = mapped { emit(app, chat_id, turn_id, mapped); }
                }
                Err(_) => break,
            },
            result = &mut send => {
                match result {
                    Ok(Some(event)) => {
                        let final_text = event.data["content"].as_str().unwrap_or(&answer).to_string();
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
    crate::mcp_manage::servers_for(cwd)
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

/// Enforces AgentDeck's existing per-tool approval pattern for Copilot SDK permission requests.
struct CopilotPermissions {
    app: AppHandle,
    chat_id: String,
    turn_id: String,
    allowed: Vec<String>,
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
        let permission_key = format!("{kind}\0{tool}\0{summary}");
        let digest = sha2::Sha256::digest(permission_key.as_bytes());
        let digest = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let allow = format!("copilot:{kind}:{digest}");
        if self.allowed.contains(&allow) {
            return PermissionResult::approve_once();
        }
        emit(
            &self.app,
            &self.chat_id,
            &self.turn_id,
            AgentEvent::PermissionDenied {
                requests: vec![ag_adapters::agent::PermissionRequest {
                    id: request_id.to_string(),
                    tool,
                    summary,
                    allow,
                }],
            },
        );
        PermissionResult::reject(None)
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
