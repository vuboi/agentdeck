//! WorkBuddy account connector: device login, token refresh, and authenticated model discovery.

use std::time::Duration;

use serde_json::Value;

use super::{secret_load, secret_save, Login};

const LEGACY_SECRET: &str = "agentdeck:workbuddy";
const ACCESS_SECRET: &str = "agentdeck:workbuddy:access";
const REFRESH_SECRET: &str = "agentdeck:workbuddy:refresh";
const EXPIRY_SECRET: &str = "agentdeck:workbuddy:expiry";
const ACCESS_COUNT_SECRET: &str = "agentdeck:workbuddy:access:count";
const REFRESH_COUNT_SECRET: &str = "agentdeck:workbuddy:refresh:count";
const BASE: &str = "https://www.workbuddy.ai";
const SECRET_CHUNK_UTF16: usize = 1800;

fn load_value(prefix: &str, count_target: &str) -> Option<String> {
    let count = secret_load(count_target.to_string()).and_then(|value| value.parse::<usize>().ok());
    if let Some(count) = count {
        let mut value = String::new();
        for index in 0..count {
            value.push_str(&secret_load(format!("{prefix}:{index}"))?);
        }
        return Some(value);
    }
    secret_load(prefix.to_string())
}

fn save_value(prefix: &str, count_target: &str, value: &str) -> Result<(), String> {
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    let mut chunk_units = 0;
    for character in value.chars() {
        let units = character.len_utf16();
        if chunk_units + units > SECRET_CHUNK_UTF16 && !chunk.is_empty() {
            chunks.push(std::mem::take(&mut chunk));
            chunk_units = 0;
        }
        chunk.push(character);
        chunk_units += units;
    }
    if !chunk.is_empty() || value.is_empty() {
        chunks.push(chunk);
    }
    for (index, chunk) in chunks.iter().enumerate() {
        secret_save(format!("{prefix}:{index}"), chunk.clone())?;
    }
    secret_save(count_target.to_string(), chunks.len().to_string())
}

pub(super) fn login_status() -> Login {
    let logged_in = load_value(ACCESS_SECRET, ACCESS_COUNT_SECRET).is_some()
        || secret_load(LEGACY_SECRET.to_string()).is_some();
    Login {
        logged_in: Some(logged_in),
        detail: Some("device sign-in".to_string()),
        ..Login::default()
    }
}

pub(crate) async fn access_token() -> Result<String, String> {
    let (access, refresh, expires_at) =
        if let Some(access) = load_value(ACCESS_SECRET, ACCESS_COUNT_SECRET) {
            let refresh = load_value(REFRESH_SECRET, REFRESH_COUNT_SECRET).unwrap_or_default();
            let expires_at = secret_load(EXPIRY_SECRET.to_string())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(u64::MAX);
            (access, refresh, expires_at)
        } else {
            let raw = secret_load(LEGACY_SECRET.to_string()).ok_or("WorkBuddy is not signed in")?;
            let json: Value =
                serde_json::from_str(&raw).map_err(|_| "saved WorkBuddy credential is invalid")?;
            (
                json["access_token"].as_str().unwrap_or_default().to_owned(),
                json["refresh_token"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                json["expires_at"].as_u64().unwrap_or(u64::MAX),
            )
        };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    if expires_at <= now + 60 && !refresh.is_empty() {
        if let Ok(token) = refresh_token(&refresh).await {
            return Ok(token);
        }
    }
    if access.is_empty() {
        Err("saved WorkBuddy credential has no access token".to_string())
    } else {
        Ok(access)
    }
}

fn save_tokens(access: &str, refresh: &str, expires_at: u64) -> Result<(), String> {
    save_value(ACCESS_SECRET, ACCESS_COUNT_SECRET, access)?;
    save_value(REFRESH_SECRET, REFRESH_COUNT_SECRET, refresh)?;
    secret_save(EXPIRY_SECRET.to_string(), expires_at.to_string())
}

async fn refresh_token(refresh_token: &str) -> Result<String, String> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("WorkBuddy/5.5.6")
        .build()
        .map_err(|error| error.to_string())?
        .post(format!("{BASE}/v2/plugin/auth/token/refresh"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("X-Refresh-Token", refresh_token)
        .header("X-Product", "SaaS")
        .header("X-Domain", "www.workbuddy.ai")
        .body("{}")
        .send()
        .await
        .map_err(|error| format!("WorkBuddy token refresh failed: {error}"))?;
    let payload: Value = response.json().await.map_err(|error| error.to_string())?;
    if payload["code"].as_i64() != Some(0) {
        return Err(payload["msg"]
            .as_str()
            .unwrap_or("refresh rejected")
            .to_string());
    }
    let access = payload["data"]["accessToken"]
        .as_str()
        .ok_or("WorkBuddy refresh returned no access token")?;
    let next_refresh = payload["data"]["refreshToken"]
        .as_str()
        .unwrap_or(refresh_token);
    let expires_in = payload["data"]["expiresIn"].as_u64().unwrap_or(2_592_000);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    save_tokens(access, next_refresh, now.saturating_add(expires_in))?;
    Ok(access.to_string())
}

pub(super) async fn sign_in() -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("WorkBuddy/5.5.6")
        .build()
        .map_err(|error| error.to_string())?;
    let state = client
        .post(format!("{BASE}/v2/plugin/auth/state?platform=CLI"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("X-Product", "SaaS")
        .json(&serde_json::json!({ "platform": "CLI" }))
        .send()
        .await
        .map_err(|error| format!("WorkBuddy sign-in could not start: {error}"))?;
    let status = state.status();
    let payload: Value = state
        .json()
        .await
        .map_err(|error| format!("WorkBuddy sent invalid sign-in data: {error}"))?;
    if !status.is_success() || payload["code"].as_i64() != Some(0) {
        return Err(format!(
            "WorkBuddy sign-in rejected: {}",
            payload["msg"].as_str().unwrap_or("unknown error")
        ));
    }
    let state_id = payload["data"]["state"]
        .as_str()
        .ok_or("WorkBuddy sign-in returned no state")?;
    let auth_url = payload["data"]["authUrl"]
        .as_str()
        .or_else(|| payload["data"]["url"].as_str())
        .ok_or("WorkBuddy sign-in returned no browser URL")?;
    crate::browser::open_external(auth_url.to_string())?;

    for _ in 0..120 {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let response = client
            .get(format!("{BASE}/v2/plugin/auth/token?state={state_id}"))
            .header("Accept", "application/json")
            .header("X-Product", "SaaS")
            .send()
            .await
            .map_err(|error| format!("WorkBuddy sign-in polling failed: {error}"))?;
        let payload: Value = response
            .json()
            .await
            .map_err(|error| format!("WorkBuddy sent invalid token data: {error}"))?;
        if payload["code"].as_i64() == Some(0) {
            let access = payload["data"]["accessToken"]
                .as_str()
                .ok_or("WorkBuddy sign-in returned no access token")?;
            let expires_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs())
                .saturating_add(payload["data"]["expiresIn"].as_u64().unwrap_or(2_592_000));
            save_tokens(
                access,
                payload["data"]["refreshToken"].as_str().unwrap_or_default(),
                expires_at,
            )?;
            return Ok(());
        }
        if payload["code"].as_i64() != Some(11217) {
            return Err(format!(
                "WorkBuddy sign-in failed: {}",
                payload["msg"].as_str().unwrap_or("unknown error")
            ));
        }
    }
    Err("WorkBuddy sign-in timed out after 10 minutes".to_string())
}

fn collect_models(value: &Value, models: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for key in ["id", "model", "modelId", "model_id"] {
                if let Some(model) = object[key].as_str().filter(|model| model.contains('-')) {
                    if !models.iter().any(|known| known == model) {
                        models.push(model.to_string());
                    }
                }
            }
            for child in object.values() {
                collect_models(child, models);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_models(item, models);
            }
        }
        _ => {}
    }
}

pub(crate) async fn models() -> Result<Vec<String>, String> {
    let token = access_token().await?;
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("WorkBuddy/5.5.6")
        .build()
        .map_err(|error| error.to_string())?
        .get(format!("{BASE}/v3/config"))
        .bearer_auth(token)
        .header("Accept", "application/json")
        .header("X-Product", "SaaS")
        .send()
        .await
        .map_err(|error| format!("WorkBuddy model catalog failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "WorkBuddy model catalog returned {}",
            response.status()
        ));
    }
    let payload: Value = response.json().await.map_err(|error| error.to_string())?;
    let mut models = Vec::new();
    collect_models(&payload, &mut models);
    models.retain(|model| !model.contains('/') && model.len() < 120);
    if models.is_empty() {
        return Err("WorkBuddy returned no chat models".to_string());
    }
    Ok(models)
}
