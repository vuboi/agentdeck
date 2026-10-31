//! OpenAI Chat Completions, OpenAI Responses, and Anthropic Messages adapters.

use std::time::Duration;

use ag_adapters::agent::AgentEvent;
use serde_json::{json, Value};

use super::{chat_url, Turn, STALL};
// ---------------------------------------------------------------------------------------------------------------
// One model call

#[derive(Default)]
pub(crate) struct Call {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) arguments: String,
    /// Responses API item id used while arguments arrive in separate SSE events.
    pub(crate) stream_id: String,
}

#[derive(Default)]
pub(crate) struct Step {
    /// Why the model stopped: `stop`, `tool_calls`, `length`…
    pub(crate) finish: String,
    pub(crate) text: String,
    /// Reasoning content that strict OpenAI-compatible providers require on the next tool turn.
    pub(crate) reasoning: String,
    pub(crate) calls: Vec<Call>,
    /// Prompt tokens not served from the provider's prompt cache.
    pub(crate) input_tokens: u64,
    /// Prompt tokens the provider read from its cache (billed at a fraction, and a sign the prefix stayed stable).
    pub(crate) cached_tokens: u64,
    pub(crate) output_tokens: u64,
}

/// Reads one `data:` line of the stream: emits text and reasoning, collects the tool calls.
pub(crate) fn read_chunk(data: &str, step: &mut Step, emit: &mut impl FnMut(AgentEvent)) {
    let Ok(json) = serde_json::from_str::<Value>(data) else {
        return;
    };
    let delta = &json["choices"][0]["delta"];
    for key in ["reasoning_content", "reasoning"] {
        if let Some(part) = delta[key].as_str().filter(|part| !part.is_empty()) {
            step.reasoning.push_str(part);
            emit(AgentEvent::Thinking {
                text: part.to_owned(),
            });
            break;
        }
    }
    if let Some(part) = delta["content"].as_str().filter(|part| !part.is_empty()) {
        step.text.push_str(part);
        emit(AgentEvent::Delta {
            text: part.to_owned(),
        });
    }
    if let Some(finish) = json["choices"][0]["finish_reason"].as_str() {
        step.finish = finish.to_owned();
    }
    for piece in delta["tool_calls"].as_array().into_iter().flatten() {
        let index = piece["index"].as_u64().unwrap_or(0) as usize;
        while step.calls.len() <= index {
            step.calls.push(Call::default());
        }
        let call = &mut step.calls[index];
        if let Some(id) = piece["id"].as_str().filter(|id| !id.is_empty()) {
            call.id = id.to_owned();
        }
        if let Some(name) = piece["function"]["name"].as_str() {
            call.name.push_str(name);
        }
        if let Some(arguments) = piece["function"]["arguments"].as_str() {
            call.arguments.push_str(arguments);
        }
    }
    if let Some(usage) = json.get("usage").filter(|usage| usage.is_object()) {
        let prompt = usage["prompt_tokens"].as_u64().unwrap_or(0);
        // OpenAI names the cached part `prompt_tokens_details.cached_tokens`; DeepSeek `prompt_cache_hit_tokens`.
        let cached = usage["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .or_else(|| usage["prompt_cache_hit_tokens"].as_u64())
            .unwrap_or(0)
            .min(prompt);
        step.input_tokens = prompt - cached;
        step.cached_tokens = cached;
        step.output_tokens = usage["completion_tokens"].as_u64().unwrap_or(0);
    }
}

pub(crate) fn responses_provider(turn: &Turn) -> bool {
    let base = turn.base_url.to_ascii_lowercase();
    (base.contains("api.commandcode.ai/provider") || base.contains("api.meta.ai/v1"))
        && !turn.model.to_ascii_lowercase().contains("claude")
}

pub(crate) fn messages_provider(turn: &Turn) -> bool {
    let base = turn.base_url.to_ascii_lowercase();
    let model = turn.model.to_ascii_lowercase();
    (base.contains("api.commandcode.ai/provider") || base.contains("api.meta.ai/v1"))
        && model.contains("claude")
}

pub(crate) fn responses_url(base: &str) -> String {
    format!("{}/responses", base.trim().trim_end_matches('/'))
}

fn messages_url(base: &str) -> String {
    format!("{}/messages", base.trim().trim_end_matches('/'))
}

pub(crate) fn provider_key(turn: &Turn) -> Option<String> {
    turn.api_key
        .clone()
        .filter(|key| !key.is_empty())
        .or_else(|| {
            turn.key_env
                .as_deref()
                .filter(|name| !name.is_empty())
                .and_then(|name| std::env::var(name).ok())
                .filter(|key| !key.is_empty())
        })
}

/// Convert the chat-completions tool shape used by AgentDeck to the flat function shape accepted by Responses.
pub(crate) fn responses_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .filter_map(|tool| {
            let function = tool.get("function")?;
            Some(json!({
                "type": "function",
                "name": function["name"],
                "description": function["description"],
                "parameters": function["parameters"],
            }))
        })
        .collect()
}

pub(crate) fn responses_content(content: &Value) -> Value {
    let Some(parts) = content.as_array() else {
        return content.clone();
    };
    Value::Array(
        parts
            .iter()
            .map(|part| match part["type"].as_str() {
                Some("text") => json!({ "type": "input_text", "text": part["text"] }),
                Some("image_url") => json!({
                    "type": "input_image",
                    "image_url": part["image_url"]["url"]
                }),
                _ => part.clone(),
            })
            .collect(),
    )
}

/// Convert the internal OpenAI-shaped transcript to Responses input items.
pub(crate) fn responses_input(messages: &[Value]) -> Vec<Value> {
    let mut input = Vec::new();
    for message in messages {
        if let Some(calls) = message["tool_calls"].as_array() {
            if !message["content"].as_str().unwrap_or_default().is_empty() {
                input.push(json!({
                    "role": "assistant",
                    "content": responses_content(&message["content"]),
                }));
            }
            for call in calls {
                let function = &call["function"];
                let id = call["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .unwrap_or("call_unknown");
                input.push(json!({
                    "type": "function_call",
                    "call_id": id,
                    "name": function["name"],
                    "arguments": function["arguments"].as_str().unwrap_or("{}"),
                }));
            }
            continue;
        }
        if message["role"].as_str() == Some("tool") {
            input.push(json!({
                "type": "function_call_output",
                "call_id": message["tool_call_id"],
                "output": message["content"],
            }));
            continue;
        }
        input.push(json!({
            "role": message["role"],
            "content": responses_content(&message["content"]),
        }));
    }
    input
}

#[derive(Debug)]
struct ResponsesAttemptError {
    message: String,
    retry_chat: bool,
}

fn response_error(response: &Value, status: reqwest::StatusCode) -> ResponsesAttemptError {
    let detail = response["error"]["message"]
        .as_str()
        .or_else(|| response["error"].as_str())
        .unwrap_or_default()
        .trim()
        .to_owned();
    let retry_chat = matches!(
        status,
        reqwest::StatusCode::BAD_REQUEST
            | reqwest::StatusCode::NOT_FOUND
            | reqwest::StatusCode::METHOD_NOT_ALLOWED
    ) && (detail.is_empty()
        || detail.to_ascii_lowercase().contains("endpoint")
        || detail.to_ascii_lowercase().contains("responses")
        || detail.to_ascii_lowercase().contains("unsupported"));
    ResponsesAttemptError {
        message: if detail.is_empty() {
            format!("the server answered {status}")
        } else {
            format!("{status}: {detail}")
        },
        retry_chat,
    }
}

fn anthropic_content(content: &Value) -> Value {
    let Some(parts) = content.as_array() else {
        return content.clone();
    };
    Value::Array(
        parts
            .iter()
            .filter_map(|part| match part["type"].as_str() {
                Some("text") => Some(json!({
                    "type": "text",
                    "text": part["text"],
                })),
                Some("image_url") => {
                    let url = part["image_url"]["url"].as_str()?;
                    let (header, data) = url.split_once(',')?;
                    let media_type = header.strip_prefix("data:")?.strip_suffix(";base64")?;
                    Some(json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data,
                        },
                    }))
                }
                _ => None,
            })
            .collect(),
    )
}

fn append_anthropic_message(messages: &mut Vec<Value>, role: &str, content: Value) {
    if let Some(previous) = messages
        .last_mut()
        .filter(|message| message["role"] == role)
    {
        let previous_content = previous["content"].take();
        let mut merged = match previous_content {
            Value::Array(parts) => parts,
            value => vec![value],
        };
        match content {
            Value::Array(parts) => merged.extend(parts),
            value => merged.push(value),
        }
        previous["content"] = Value::Array(merged);
    } else {
        messages.push(json!({ "role": role, "content": content }));
    }
}

fn anthropic_messages(messages: &[Value]) -> (String, Vec<Value>) {
    let mut system = String::new();
    let mut result = Vec::new();
    for message in messages {
        match message["role"].as_str().unwrap_or_default() {
            "system" => {
                let text = message["content"].as_str().unwrap_or_default();
                if !system.is_empty() && !text.is_empty() {
                    system.push('\n');
                }
                system.push_str(text);
            }
            "tool" => append_anthropic_message(
                &mut result,
                "user",
                json!([{
                    "type": "tool_result",
                    "tool_use_id": message["tool_call_id"],
                    "content": message["content"],
                }]),
            ),
            role => {
                let mut content = Vec::new();
                if let Some(text) = message["content"].as_str().filter(|text| !text.is_empty()) {
                    content.push(json!({ "type": "text", "text": text }));
                } else if message["content"].is_array() {
                    content = anthropic_content(&message["content"])
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                }
                for call in message["tool_calls"].as_array().into_iter().flatten() {
                    let function = &call["function"];
                    let input = function["arguments"]
                        .as_str()
                        .and_then(|arguments| serde_json::from_str(arguments).ok())
                        .unwrap_or_else(|| json!({}));
                    content.push(json!({
                        "type": "tool_use",
                        "id": call["id"],
                        "name": function["name"],
                        "input": input,
                    }));
                }
                if !content.is_empty() {
                    append_anthropic_message(&mut result, role, Value::Array(content));
                }
            }
        }
    }
    (system, result)
}

fn messages_tools(tools: &[Value]) -> Vec<Value> {
    tools
        .iter()
        .filter_map(|tool| {
            let function = tool.get("function")?;
            Some(json!({
                "name": function["name"],
                "description": function["description"],
                "input_schema": function["parameters"],
            }))
        })
        .collect()
}

pub(crate) fn read_messages_chunk(data: &str, step: &mut Step, emit: &mut impl FnMut(AgentEvent)) {
    let Ok(json) = serde_json::from_str::<Value>(data) else {
        return;
    };
    match json["type"].as_str().unwrap_or_default() {
        "content_block_start" => {
            if json["content_block"]["type"].as_str() == Some("tool_use") {
                step.calls.push(Call {
                    id: json["content_block"]["id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    name: json["content_block"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                    arguments: String::new(),
                    stream_id: json["index"].as_u64().unwrap_or(0).to_string(),
                });
            }
        }
        "content_block_delta" => {
            let delta = &json["delta"];
            match delta["type"].as_str().unwrap_or_default() {
                "text_delta" => {
                    if let Some(text) = delta["text"].as_str().filter(|text| !text.is_empty()) {
                        step.text.push_str(text);
                        emit(AgentEvent::Delta {
                            text: text.to_owned(),
                        });
                    }
                }
                "thinking_delta" => {
                    if let Some(text) = delta["thinking"].as_str().filter(|text| !text.is_empty()) {
                        step.reasoning.push_str(text);
                        emit(AgentEvent::Thinking {
                            text: text.to_owned(),
                        });
                    }
                }
                "input_json_delta" => {
                    let index = json["index"].as_u64().unwrap_or(0).to_string();
                    if let Some(call) = step.calls.iter_mut().find(|call| call.stream_id == index) {
                        call.arguments
                            .push_str(delta["partial_json"].as_str().unwrap_or_default());
                    }
                }
                _ => {}
            }
        }
        "message_delta" => {
            if let Some(reason) = json["delta"]["stop_reason"].as_str() {
                step.finish = if reason == "tool_use" {
                    "tool_calls"
                } else {
                    reason
                }
                .to_owned();
            }
            let usage = &json["usage"];
            step.output_tokens = usage["output_tokens"]
                .as_u64()
                .unwrap_or(step.output_tokens);
        }
        "message_start" => {
            step.input_tokens = json["message"]["usage"]["input_tokens"]
                .as_u64()
                .unwrap_or(0);
        }
        "error" => {
            step.finish = "error".into();
            step.text = json["error"]["message"]
                .as_str()
                .unwrap_or("Anthropic Messages API returned an error")
                .to_owned();
        }
        _ => {}
    }
}

async fn model_call_messages(
    client: &reqwest::Client,
    turn: &Turn,
    messages: &[Value],
    tools: &[Value],
    emit: &mut impl FnMut(AgentEvent),
) -> Result<Step, String> {
    let (system, messages) = anthropic_messages(messages);
    let mut body = json!({
        "model": turn.model,
        "max_tokens": 8192,
        "stream": true,
        "messages": messages,
    });
    if !system.is_empty() {
        body["system"] = json!(system);
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(messages_tools(tools));
    }
    match turn.effort.as_deref() {
        Some("off") => body["thinking"] = json!({ "type": "disabled" }),
        Some("low") => body["thinking"] = json!({ "type": "enabled", "budget_tokens": 2048 }),
        Some("medium") => body["thinking"] = json!({ "type": "enabled", "budget_tokens": 4096 }),
        Some("high") => body["thinking"] = json!({ "type": "enabled", "budget_tokens": 8192 }),
        _ => {}
    }
    let mut request = client
        .post(messages_url(&turn.base_url))
        .header("anthropic-version", "2023-06-01")
        .json(&body);
    if let Some(key) = provider_key(turn) {
        request = request.header("x-api-key", key);
    }
    let sent = tokio::time::timeout(Duration::from_secs(300), request.send())
        .await
        .map_err(|_| "the server did not answer within 5 minutes".to_owned())?
        .map_err(|error| error.to_string())?;
    if !sent.status().is_success() {
        let status = sent.status();
        let detail = sent.text().await.unwrap_or_default();
        return Err(if detail.trim().is_empty() {
            format!("the server answered {status}")
        } else {
            format!("{status}: {detail}")
        });
    }
    let mut response = sent;
    let mut step = Step::default();
    let mut pending = Vec::new();
    loop {
        let next = tokio::time::timeout(STALL, response.chunk())
            .await
            .map_err(|_| {
                format!(
                    "the model sent nothing for {} seconds and was stopped",
                    STALL.as_secs()
                )
            })?
            .map_err(|error| error.to_string())?;
        match next {
            Some(bytes) => {
                pending.extend_from_slice(&bytes);
                while let Some(line) = take_line(&mut pending) {
                    if let Some(data) = line.trim().strip_prefix("data:").map(str::trim) {
                        read_messages_chunk(data, &mut step, emit);
                    }
                }
            }
            None => {
                if !pending.is_empty() {
                    if let Some(data) = String::from_utf8_lossy(&pending)
                        .trim()
                        .strip_prefix("data:")
                        .map(str::trim)
                    {
                        read_messages_chunk(data, &mut step, emit);
                    }
                }
                break;
            }
        }
    }
    if step.finish == "error" {
        return Err(step.text);
    }
    if step.finish.is_empty() {
        step.finish = if step.calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        }
        .into();
    }
    Ok(step)
}

pub(crate) fn read_responses_chunk(data: &str, step: &mut Step, emit: &mut impl FnMut(AgentEvent)) {
    let Ok(json) = serde_json::from_str::<Value>(data) else {
        return;
    };
    match json["type"].as_str().unwrap_or_default() {
        "response.output_text.delta" => {
            if let Some(text) = json["delta"].as_str().filter(|text| !text.is_empty()) {
                step.text.push_str(text);
                emit(AgentEvent::Delta {
                    text: text.to_owned(),
                });
            }
        }
        "response.reasoning_summary_text.delta"
        | "response.reasoning_text.delta"
        | "response.reasoning_content.delta" => {
            if let Some(text) = json["delta"].as_str().filter(|text| !text.is_empty()) {
                emit(AgentEvent::Thinking {
                    text: text.to_owned(),
                });
            }
        }
        "response.output_item.added" => {
            let item = &json["item"];
            if item["type"].as_str() == Some("function_call") {
                let mut call = Call {
                    id: item["call_id"].as_str().unwrap_or_default().to_owned(),
                    name: item["name"].as_str().unwrap_or_default().to_owned(),
                    arguments: item["arguments"].as_str().unwrap_or_default().to_owned(),
                    stream_id: item["id"].as_str().unwrap_or_default().to_owned(),
                };
                if call.id.is_empty() {
                    call.id = call.stream_id.clone();
                }
                step.calls.push(call);
            }
        }
        "response.function_call_arguments.delta" => {
            let id = json["item_id"].as_str().unwrap_or_default();
            if let Some(call) = step
                .calls
                .iter_mut()
                .find(|call| call.stream_id == id || call.id == id)
            {
                if let Some(delta) = json["delta"].as_str() {
                    call.arguments.push_str(delta);
                }
            }
        }
        "response.output_item.done" => {
            let item = &json["item"];
            if item["type"].as_str() == Some("function_call") {
                let id = item["id"].as_str().unwrap_or_default();
                if let Some(call) = step.calls.iter_mut().find(|call| {
                    call.stream_id == id || call.id == item["call_id"].as_str().unwrap_or_default()
                }) {
                    if let Some(arguments) = item["arguments"].as_str() {
                        call.arguments = arguments.to_owned();
                    }
                }
            }
        }
        "response.completed" => {
            step.finish = if step.calls.is_empty() {
                "stop"
            } else {
                "tool_calls"
            }
            .to_owned();
            let usage = &json["response"]["usage"];
            step.input_tokens = usage["input_tokens"].as_u64().unwrap_or(0);
            step.cached_tokens = usage["input_tokens_details"]["cached_tokens"]
                .as_u64()
                .unwrap_or(0)
                .min(step.input_tokens);
            step.input_tokens -= step.cached_tokens;
            step.output_tokens = usage["output_tokens"].as_u64().unwrap_or(0);
        }
        _ => {}
    }
}

async fn model_call_responses(
    client: &reqwest::Client,
    turn: &Turn,
    messages: &[Value],
    tools: &[Value],
    emit: &mut impl FnMut(AgentEvent),
) -> Result<Step, ResponsesAttemptError> {
    let mut body = json!({
        "model": turn.model,
        "stream": true,
        "input": responses_input(messages),
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(responses_tools(tools));
    }
    match turn.effort.as_deref() {
        Some("off") => body["reasoning"] = json!({ "effort": "none" }),
        Some(level @ ("low" | "medium" | "high")) => body["reasoning"] = json!({ "effort": level }),
        _ => {}
    }
    let mut request = client.post(responses_url(&turn.base_url)).json(&body);
    if let Some(key) = provider_key(turn) {
        request = request.bearer_auth(key);
    }
    let sent = match tokio::time::timeout(Duration::from_secs(300), request.send()).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            return Err(ResponsesAttemptError {
                message: error.to_string(),
                retry_chat: error.is_status(),
            })
        }
        Err(_) => {
            return Err(ResponsesAttemptError {
                message: "the server did not answer within 5 minutes".into(),
                retry_chat: false,
            })
        }
    };
    if !sent.status().is_success() {
        let status = sent.status();
        let body = sent.text().await.unwrap_or_default();
        let json = serde_json::from_str::<Value>(&body).unwrap_or_default();
        return Err(response_error(&json, status));
    }
    let mut response = sent;
    let mut step = Step::default();
    let mut pending = Vec::new();
    'stream: loop {
        let next = match tokio::time::timeout(STALL, response.chunk()).await {
            Ok(next) => next,
            Err(_) => {
                return Err(ResponsesAttemptError {
                    message: format!(
                        "the model sent nothing for {} seconds and was stopped",
                        STALL.as_secs()
                    ),
                    retry_chat: false,
                })
            }
        };
        match next {
            Ok(Some(bytes)) => {
                pending.extend_from_slice(&bytes);
                while let Some(line) = take_line(&mut pending) {
                    let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else {
                        continue;
                    };
                    if data == "[DONE]" {
                        break 'stream;
                    }
                    read_responses_chunk(data, &mut step, emit);
                }
            }
            Ok(None) => {
                if !pending.is_empty() {
                    if let Some(data) = String::from_utf8_lossy(&pending)
                        .trim()
                        .strip_prefix("data:")
                        .map(str::trim)
                    {
                        read_responses_chunk(data, &mut step, emit);
                    }
                }
                break;
            }
            Err(error) => {
                return Err(ResponsesAttemptError {
                    message: error.to_string(),
                    retry_chat: false,
                })
            }
        }
    }
    Ok(step)
}

/// Sends the conversation and streams the answer. `Err` is a message for the user.
pub(crate) async fn model_call(
    client: &reqwest::Client,
    turn: &Turn,
    messages: &[Value],
    tools: &[Value],
    emit: &mut impl FnMut(AgentEvent),
) -> Result<Step, String> {
    let mut outgoing_messages = messages.to_vec();
    if turn.workbuddy {
        outgoing_messages = crate::providers::workbuddy::prepare_messages(&outgoing_messages);
    }
    if messages_provider(turn) {
        return model_call_messages(client, turn, &outgoing_messages, tools, emit).await;
    }
    if responses_provider(turn) {
        match model_call_responses(client, turn, &outgoing_messages, tools, emit).await {
            Ok(step) => return Ok(step),
            Err(error) if error.retry_chat => {}
            Err(error) => return Err(error.message),
        }
    }
    let mut body = json!({
        "model": turn.model,
        "stream": true,
        "stream_options": { "include_usage": true },
        "messages": outgoing_messages,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools.to_vec());
    }
    // Workers AI cuts an answer short at its own small default, so ask for a fixed ceiling.
    if turn.base_url.contains("api.cloudflare.com") {
        body["max_tokens"] = json!(2048);
    }
    // Servers differ in how they take this, and one ignores what it does not know: `reasoning_effort` (LM Studio,
    // OpenAI-style, Ollama), `chat_template_kwargs` (llama.cpp, vLLM) and `think` (Ollama).
    match turn.effort.as_deref() {
        Some("off") => {
            body["reasoning_effort"] = json!("none");
            body["chat_template_kwargs"] = json!({ "enable_thinking": false });
            body["think"] = json!(false);
        }
        Some(level @ ("low" | "medium" | "high")) => body["reasoning_effort"] = json!(level),
        Some(_) => body["reasoning_effort"] = json!("high"),
        None => {}
    }
    let mut request = client
        .post(chat_url(&turn.kind, &turn.base_url))
        .json(&body);
    if turn.workbuddy {
        request = crate::providers::workbuddy::authorize(request, turn.api_key.as_deref()).await?;
    } else if let Some(key) = provider_key(turn) {
        request = request.bearer_auth(key);
    }
    // A local model reads a long prompt before it answers, so the wait for the first byte is generous but not endless.
    let sent = match tokio::time::timeout(Duration::from_secs(300), request.send()).await {
        Ok(sent) => sent,
        Err(_) => return Err("the server did not answer within 5 minutes".into()),
    };
    let mut response = match sent {
        Ok(response) => response,
        Err(error) if error.is_connect() => {
            return Err("could not connect: is the server running?".into())
        }
        Err(error) => return Err(error.to_string()),
    };
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<Value>(&detail)
            .ok()
            .and_then(|json| json["error"]["message"].as_str().map(str::to_owned))
            .unwrap_or(detail);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            format!("the server answered {status}")
        } else {
            format!("{status}: {detail}")
        });
    }
    let mut step = Step::default();
    // Buffer raw bytes: a network chunk can end inside a Vietnamese character or emoji.
    // Decode only complete lines; ASCII newlines cannot occur inside a multi-byte UTF-8 character.
    let mut pending: Vec<u8> = Vec::new();
    'stream: loop {
        // A server that stops sending (a stuck model, a dropped connection) must not leave the chat waiting for ever.
        let next = match tokio::time::timeout(STALL, response.chunk()).await {
            Ok(next) => next,
            Err(_) => {
                return Err(format!(
                    "the model sent nothing for {} seconds and was stopped",
                    STALL.as_secs()
                ))
            }
        };
        match next {
            Ok(Some(bytes)) => {
                pending.extend_from_slice(&bytes);
                while let Some(line) = take_line(&mut pending) {
                    if handle_sse_line(&line, &mut step, emit) {
                        break 'stream;
                    }
                }
            }
            Ok(None) => {
                // The stream can end on a line without its newline: that last `data:` line still holds text.
                if !pending.is_empty() {
                    let rest = String::from_utf8_lossy(&pending).into_owned();
                    handle_sse_line(&rest, &mut step, emit);
                }
                break;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(step)
}

/// Reads one SSE line. `true` when it is the end of the stream (`data: [DONE]`).
pub(crate) fn handle_sse_line(
    line: &str,
    step: &mut Step,
    emit: &mut impl FnMut(AgentEvent),
) -> bool {
    let Some(data) = line.trim().strip_prefix("data:") else {
        return false;
    };
    let data = data.trim();
    if data == "[DONE]" {
        return true;
    }
    read_chunk(data, step, emit);
    false
}

/// The next complete SSE line (including its newline), decoded as UTF-8; incomplete bytes stay buffered.
pub(crate) fn take_line(pending: &mut Vec<u8>) -> Option<String> {
    let end = pending.iter().position(|byte| *byte == b'\n')?;
    let line: Vec<u8> = pending.drain(..=end).collect();
    Some(String::from_utf8_lossy(&line).into_owned())
}
