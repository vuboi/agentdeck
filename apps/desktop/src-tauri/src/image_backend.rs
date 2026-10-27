//! The image generators the image panel can use. Each one is an `ImageBackend`; `generate` in `image_gen.rs` only looks
//! the backend up by the id a model carries and hands it one job, so a new provider is one more `impl` and one more line
//! in `BACKENDS`, with nothing else in the run loop changing.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::image_gen::{generate_one, imagen_script, trim_error, variation_hint, CustomSuggester, ImageConfig, CANCELLED};

/// One candidate to make: the prompt (with the picked settings), which model, and where the file goes.
pub struct Job<'a> {
    pub prompt: &'a str,
    pub variant: u8,
    pub model: &'a str,
    pub output: &'a Path,
    pub references: &'a [PathBuf],
    pub config: &'a ImageConfig,
    /// The provider (address and key) for a backend that calls an API of its own.
    pub provider: Option<&'a CustomSuggester>,
}

pub trait ImageBackend: Sync {
    /// The id a model carries (`backend` in the panel's model list).
    fn id(&self) -> &'static str;
    /// Makes the candidate and returns the saved file paths.
    fn generate(&self, job: &Job) -> Result<Vec<String>, String>;
}

/// The Codex image script, on ChatGPT's Responses (`responses`) or Images (`images`) endpoint.
struct CodexScript(&'static str);

impl ImageBackend for CodexScript {
    fn id(&self) -> &'static str {
        self.0
    }

    fn generate(&self, job: &Job) -> Result<Vec<String>, String> {
        let script = imagen_script()?;
        generate_one(&script, job.prompt, job.variant, job.model, self.0, job.output, job.references)
    }
}

/// Google's Nano Banana models through the Interactions API, on the key of the Gemini provider.
struct Gemini;

impl ImageBackend for Gemini {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn generate(&self, job: &Job) -> Result<Vec<String>, String> {
        use base64::Engine;
        if CANCELLED.load(Ordering::SeqCst) {
            return Err("stopped".to_string());
        }
        let provider = job.provider.ok_or("the Gemini provider is not set up")?;
        let key = provider_key(provider).ok_or("add the Gemini API key in Settings, Cloud providers")?;
        let mut input = vec![serde_json::json!({
            "type": "text",
            "text": format!("Generate exactly one image. {}\n\n{}", job.prompt, variation_hint(job.variant)),
        })];
        for reference in job.references {
            let mime = match reference
                .extension()
                .and_then(|ext| ext.to_str())
                .map(str::to_lowercase)
                .as_deref()
            {
                Some("jpg" | "jpeg") => "image/jpeg",
                Some("webp") => "image/webp",
                _ => "image/png",
            };
            let bytes = std::fs::read(reference).map_err(|error| error.to_string())?;
            input.push(serde_json::json!({
                "type": "image",
                "mime_type": mime,
                "data": base64::engine::general_purpose::STANDARD.encode(bytes),
            }));
        }
        let mut body = serde_json::json!({ "model": job.model, "input": input });
        if let Some(format) = gemini_format(job.config) {
            body["response_format"] = format;
        }
        let answer = tauri::async_runtime::block_on(async {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .map_err(|error| error.to_string())?;
            let response = client
                .post(gemini_url(&provider.base_url))
                .header("x-goog-api-key", key)
                .json(&body)
                .send()
                .await
                .map_err(|error| format!("could not reach Gemini: {error}"))?;
            let status = response.status();
            let text = response.text().await.map_err(|error| error.to_string())?;
            let json: serde_json::Value = serde_json::from_str(&text)
                .map_err(|_| trim_error(&format!("Gemini answered {status}: {text}")))?;
            if !status.is_success() {
                let detail = json["error"]["message"].as_str().unwrap_or("no detail");
                return Err(trim_error(&format!("Gemini answered {status}: {detail}")));
            }
            Ok::<serde_json::Value, String>(json)
        })?;
        if CANCELLED.load(Ordering::SeqCst) {
            return Err("stopped".to_string());
        }
        let data = find_image_data(&answer).ok_or("Gemini returned no image (the prompt may have been refused)")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|error| format!("Gemini's image is not valid: {error}"))?;
        std::fs::write(job.output, bytes).map_err(|error| error.to_string())?;
        Ok(vec![job.output.to_string_lossy().to_string()])
    }
}

static BACKENDS: [&dyn ImageBackend; 3] = [&CodexScript("responses"), &CodexScript("images"), &Gemini];

/// The backend that answers to `id`, if there is one.
pub fn backend_for(id: &str) -> Option<&'static dyn ImageBackend> {
    BACKENDS.iter().copied().find(|backend| backend.id() == id)
}

/// The Interactions endpoint beside the provider's OpenAI-compatible address (`.../v1beta/openai`).
fn gemini_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    let root = base.strip_suffix("/openai").unwrap_or(base);
    format!("{root}/interactions")
}

fn provider_key(provider: &CustomSuggester) -> Option<String> {
    provider
        .api_key
        .clone()
        .filter(|key| !key.is_empty())
        .or_else(|| {
            provider
                .key_env
                .as_deref()
                .filter(|name| !name.is_empty())
                .and_then(|name| std::env::var(name).ok())
        })
}

/// The picked ratio and resolution as Gemini's `response_format`; `None` when both are Auto.
fn gemini_format(config: &ImageConfig) -> Option<serde_json::Value> {
    let ratio = config
        .ratio
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let size = config.resolution.as_deref().and_then(|value| {
        if value.starts_with("4K") {
            Some("4K")
        } else if value.starts_with("2K") {
            Some("2K")
        } else if value.starts_with("Full HD") || value.starts_with("HD") {
            Some("1K")
        } else {
            None
        }
    });
    if ratio.is_none() && size.is_none() {
        return None;
    }
    let mut format = serde_json::json!({ "type": "image" });
    if let Some(ratio) = ratio {
        format["aspect_ratio"] = ratio.into();
    }
    if let Some(size) = size {
        format["image_size"] = size.into();
    }
    Some(format)
}

/// The first base64 image in an Interactions answer (`{"type":"image","data":...}`, or `inlineData` in older shapes).
fn find_image_data(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("type").and_then(|kind| kind.as_str()) == Some("image") {
                if let Some(data) = map.get("data").and_then(|data| data.as_str()) {
                    return Some(data.to_string());
                }
            }
            for key in ["inlineData", "inline_data"] {
                if let Some(data) = map.get(key).and_then(|inline| inline["data"].as_str()) {
                    return Some(data.to_string());
                }
            }
            map.values().find_map(find_image_data)
        }
        serde_json::Value::Array(items) => items.iter().find_map(find_image_data),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backends_are_found_by_id() {
        for id in ["responses", "images", "gemini"] {
            assert_eq!(backend_for(id).map(|backend| backend.id()), Some(id));
        }
        assert!(backend_for("nope").is_none());
    }

    #[test]
    fn gemini_address_and_image_are_found() {
        assert_eq!(
            gemini_url("https://generativelanguage.googleapis.com/v1beta/openai/"),
            "https://generativelanguage.googleapis.com/v1beta/interactions"
        );
        let answer = serde_json::json!({ "steps": [{ "type": "model_output", "content": [
            { "type": "text", "text": "ok" }, { "type": "image", "mime_type": "image/png", "data": "QUJD" } ] }] });
        assert_eq!(find_image_data(&answer).as_deref(), Some("QUJD"));
        assert!(find_image_data(&serde_json::json!({ "steps": [] })).is_none());
        let picked = ImageConfig {
            ratio: Some("16:9".into()),
            resolution: Some("4K (3840×2160)".into()),
            ..Default::default()
        };
        assert_eq!(gemini_format(&picked).unwrap()["image_size"], "4K");
        assert!(gemini_format(&ImageConfig::default()).is_none());
    }
}
