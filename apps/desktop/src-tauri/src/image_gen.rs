//! Small, optional bridge to the repository's codex-imagen skill.
//!
//! The desktop app keeps image generation outside the agent turn. This makes it useful for a normal chat,
//! an artifact task, or a delegated workflow without changing the existing provider adapters.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// Set by `image_stop`: the run in progress stops before its next candidate.
static CANCELLED: AtomicBool = AtomicBool::new(false);
/// Process ids of the generator runs in progress, so `image_stop` can end them.
static ACTIVE: LazyLock<Mutex<Vec<u32>>> = LazyLock::new(Default::default);
/// The app's resource folder: the codex-imagen script is bundled there (see `tauri.conf.json`).
static RESOURCE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Called once at start-up with Tauri's resource folder.
pub fn set_resource_dir(dir: PathBuf) {
    let _ = RESOURCE_DIR.set(dir);
}

const DEFAULT_VARIANTS: u8 = 1;
const MAX_VARIANTS: u8 = 4;
const MAX_MODELS: usize = 4;
const MAX_PROMPT_CHARS: usize = 30_000;
const MAX_SUGGESTIONS: usize = 7;
const MAX_KEYWORDS: usize = 12;
const SUGGEST_INSTRUCTIONS: &str = r#"You improve prompts for image generation. Reply with one JSON object and nothing else, no markdown: {"prompts": [seven distinct, ready-to-use prompts as strings], "keywords": [up to ten {"category": "...", "value": "..."}]}. Preserve the user's subject and intent. Add useful visual detail: style, composition, lighting, camera or rendering direction. The settings below are fixed: follow them exactly and do not contradict them. Each keyword is a short term that fits one setting; category is one of style, domain, audience, ratio, resolution, or keyword (a plain term to add to the prompt). Reference images, when given, show the subject or style to follow: look at each one before you write. Do not add commentary, policy text, or claims about the image."#;

#[derive(Debug, Deserialize)]
pub struct ImageModelRequest {
    pub model: String,
    pub backend: String,
}

/// What the user picked under the prompt box. `None` means "Auto": no instruction is sent for that setting.
#[derive(Debug, Default, Deserialize)]
pub struct ImageConfig {
    pub style: Option<String>,
    pub domain: Option<String>,
    pub audience: Option<String>,
    pub ratio: Option<String>,
    pub resolution: Option<String>,
}

impl ImageConfig {
    /// The picked settings as lines for the model, in a fixed order. Empty when nothing is picked.
    fn brief(&self) -> String {
        let pick = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let mut lines = Vec::new();
        if let Some(style) = pick(&self.style) {
            lines.push(format!("Style: {style}."));
        }
        if let Some(domain) = pick(&self.domain) {
            lines.push(format!("Subject domain: {domain}."));
        }
        if let Some(audience) = pick(&self.audience) {
            lines.push(format!("Audience: {audience}. Keep the content suitable for that audience; no explicit sexual content."));
        }
        if let Some(ratio) = pick(&self.ratio) {
            lines.push(format!("Aspect ratio: {ratio}."));
        }
        if let Some(resolution) = pick(&self.resolution) {
            lines.push(format!("Output resolution: {resolution}."));
        }
        lines.join("\n")
    }
}

#[derive(Debug, Deserialize)]
pub struct ImageGenerateRequest {
    pub prompt: String,
    pub models: Vec<ImageModelRequest>,
    pub variants: Option<u8>,
    #[serde(default)]
    pub reference_paths: Vec<String>,
    #[serde(default)]
    pub config: ImageConfig,
    pub cwd: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GeneratedImage {
    pub model: String,
    pub backend: String,
    pub path: String,
    pub variant: u8,
}

#[derive(Debug, Serialize)]
pub struct ImageGenerationFailure {
    pub model: String,
    pub backend: String,
    pub variant: u8,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct ImageGenerateResult {
    /// Names the run in the history and its folder.
    pub run_id: String,
    pub images: Vec<GeneratedImage>,
    pub failures: Vec<ImageGenerationFailure>,
}

#[derive(Debug, Deserialize)]
pub struct ImagePromptSuggestRequest {
    /// The CLI that writes the suggestions: `claude`, `antigravity` or `opencode`; anything else uses the Codex API.
    #[serde(default)]
    pub provider: Option<String>,
    pub prompt: String,
    pub model: String,
    #[serde(default)]
    pub reference_paths: Vec<String>,
    #[serde(default)]
    pub config: ImageConfig,
}

/// A short term the model suggests for one of the settings (or a plain keyword for the prompt).
#[derive(Debug, Serialize)]
pub struct SuggestedKeyword {
    pub category: String,
    pub value: String,
}

#[derive(Debug, Serialize)]
pub struct ImagePromptSuggestResult {
    pub model: String,
    pub suggestions: Vec<String>,
    pub keywords: Vec<SuggestedKeyword>,
}

#[tauri::command]
pub async fn image_generate(request: ImageGenerateRequest) -> Result<ImageGenerateResult, String> {
    tauri::async_runtime::spawn_blocking(move || generate(request))
        .await
        .map_err(|error| error.to_string())?
}

/// Stops the image run in progress: candidates already saved stay, the running generator process is killed.
#[tauri::command]
pub fn image_stop() {
    CANCELLED.store(true, Ordering::SeqCst);
    let pids = ACTIVE.lock().map(|pids| pids.clone()).unwrap_or_default();
    for pid in pids {
        kill_tree(pid);
    }
}

fn kill_tree(pid: u32) {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("taskkill");
        command.args(["/PID", &pid.to_string(), "/T", "/F"]);
        command
    } else {
        let mut command = Command::new("kill");
        command.args(["-9", &pid.to_string()]);
        command
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let _ = command.stdin(Stdio::null()).output();
}

#[tauri::command]
pub async fn image_prompt_suggest(
    request: ImagePromptSuggestRequest,
) -> Result<ImagePromptSuggestResult, String> {
    tauri::async_runtime::spawn_blocking(move || suggest(request))
        .await
        .map_err(|error| error.to_string())?
}

fn generate(request: ImageGenerateRequest) -> Result<ImageGenerateResult, String> {
    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        return Err("prompt is required".to_string());
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(format!(
            "prompt is longer than {MAX_PROMPT_CHARS} characters"
        ));
    }
    if request.models.is_empty() {
        return Err("choose at least one image model".to_string());
    }
    if request.models.len() > MAX_MODELS {
        return Err(format!("choose at most {MAX_MODELS} image models at once"));
    }
    let variants = request
        .variants
        .unwrap_or(DEFAULT_VARIANTS)
        .clamp(1, MAX_VARIANTS);
    CANCELLED.store(false, Ordering::SeqCst);
    let script = imagen_script()?;
    let output_root = output_root(request.cwd.as_deref())?;
    let run_id = unique_run_name();
    let run_folder = output_root.join(&run_id);
    std::fs::create_dir_all(&run_folder).map_err(|error| error.to_string())?;
    let references = validate_references(&request.reference_paths)?;
    // The picked settings travel with the prompt, so each candidate follows them.
    let brief = request.config.brief();
    let prompt_text = if brief.is_empty() {
        prompt.to_string()
    } else {
        format!("{prompt}\n\n{brief}")
    };

    let mut images = Vec::new();
    let mut failures = Vec::new();
    let selected_models = request.models;
    for model in &selected_models {
        let model_name = model.model.trim().to_string();
        let backend = model.backend.trim().to_string();
        if model_name.is_empty() || !matches!(backend.as_str(), "responses" | "images") {
            for variant in 1..=variants {
                failures.push(ImageGenerationFailure {
                    model: model_name.clone(),
                    backend: backend.clone(),
                    variant,
                    error: "invalid image model or backend".to_string(),
                });
            }
            continue;
        }
        for variant in 1..=variants {
            if CANCELLED.load(Ordering::SeqCst) {
                break;
            }
            let output = run_folder.join(format!("{}-{variant}.png", safe_name(&model_name)));
            match generate_one(
                &script,
                &prompt_text,
                variant,
                &model_name,
                &backend,
                &output,
                &references,
            ) {
                Ok(paths) => images.extend(paths.into_iter().map(|path| GeneratedImage {
                    model: model_name.clone(),
                    backend: backend.clone(),
                    path,
                    variant,
                })),
                Err(error) => failures.push(ImageGenerationFailure {
                    model: model_name.clone(),
                    backend: backend.clone(),
                    variant,
                    error,
                }),
            }
        }
    }
    let status = if was_stopped() {
        "stopped"
    } else if images.is_empty() && !failures.is_empty() {
        "failed"
    } else {
        "done"
    };
    let result = ImageGenerateResult {
        run_id,
        images,
        failures,
    };
    let models: Vec<String> = selected_models
        .iter()
        .map(|model| format!("{}:{}", model.model, model.backend))
        .collect();
    // History is best effort: a database error must not hide the images that were just made.
    if let Err(error) =
        crate::image_history::record(&result.run_id, prompt, &models, variants, status, &result)
    {
        eprintln!("image history not saved: {error}");
    }
    Ok(result)
}

/// `true` when the last run was stopped with `image_stop`.
pub fn was_stopped() -> bool {
    CANCELLED.load(Ordering::SeqCst)
}

fn suggest(request: ImagePromptSuggestRequest) -> Result<ImagePromptSuggestResult, String> {
    let prompt = request.prompt.trim();
    let model = request.model.trim();
    let references = validate_references(&request.reference_paths)?;
    if prompt.is_empty() && references.is_empty() {
        return Err("write a prompt or add a reference image".to_string());
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(format!(
            "prompt is longer than {MAX_PROMPT_CHARS} characters"
        ));
    }
    if model.is_empty() {
        return Err("choose a prompt suggestion model".to_string());
    }
    let input = suggest_input(prompt, &request.config.brief(), &references);
    let provider = request.provider.as_deref().unwrap_or_default();
    if provider == "antigravity" {
        return suggest_with_cli(
            ag_adapters::agent::runner::resolve_binary(
                &ag_adapters::agent::antigravity::Antigravity::default(),
            ),
            &["--print-timeout", "5m", "--model", model, "-p", &input],
            None,
            model,
            "Antigravity",
        );
    }
    if provider == "opencode" {
        return suggest_with_cli(
            ag_adapters::agent::runner::resolve_binary(
                &ag_adapters::agent::opencode::OpenCode::default(),
            ),
            &["run", "--standalone", "--model", model],
            Some(&input),
            model,
            "OpenCode",
        );
    }
    if provider == "claude" || model.starts_with("claude-") {
        return suggest_with_claude(&input, model, &references);
    }

    let script = imagen_script()?;
    let mut command = Command::new("node");
    command
        .arg(script)
        .args([
            "--suggest",
            "--json",
            "--quiet",
            "--no-retry",
            "--timeout",
            "120",
            "--backend",
            "responses",
            "--model",
            model,
            "--prompt",
        ])
        .arg(&input);
    for reference in &references {
        command.arg("--input-ref").arg(reference);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let result = command
        .output()
        .map_err(|error| format!("could not start prompt suggestion API: {error}"))?;
    if !result.status.success() {
        let detail = String::from_utf8_lossy(&result.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("prompt suggestion API exited with {}", result.status)
        } else {
            trim_error(&detail)
        });
    }
    let json: serde_json::Value = serde_json::from_slice(&result.stdout)
        .map_err(|error| format!("prompt suggestion API returned invalid JSON: {error}"))?;
    let (suggestions, keywords) = read_suggestions(&json);
    if suggestions.is_empty() {
        return Err("prompt suggestion API returned no suggestions".to_string());
    }
    Ok(ImagePromptSuggestResult {
        model: model.to_string(),
        suggestions,
        keywords,
    })
}

/// Claude writes the suggestions in a one-shot `claude -p` run, the input on stdin as in a chat turn. Reference
/// images are opened by its Read tool, which this run is allowed to use.
fn suggest_with_claude(
    input: &str,
    model: &str,
    references: &[PathBuf],
) -> Result<ImagePromptSuggestResult, String> {
    use std::io::Write;

    let binary = ag_adapters::agent::runner::resolve_binary(&ag_adapters::agent::claude::Claude);
    let mut command = Command::new(binary);
    command.args(["-p", "--model", model, "--output-format", "text"]);
    if !references.is_empty() {
        command.args(["--allowedTools", "Read"]);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start Claude: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    let result = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !result.status.success() {
        let detail = String::from_utf8_lossy(&result.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("Claude exited with {}", result.status)
        } else {
            trim_error(&detail)
        });
    }
    let text = String::from_utf8_lossy(&result.stdout);
    let (suggestions, keywords) = parse_suggestions(&text);
    if suggestions.is_empty() {
        return Err("Claude returned no suggestions".to_string());
    }
    Ok(ImagePromptSuggestResult {
        model: model.to_string(),
        suggestions,
        keywords,
    })
}

/// One-shot run of a coding CLI that writes the suggestions: its plain-text answer is read for the JSON object.
/// `stdin_input` is the prompt for a CLI that reads it from stdin; the others get it in `args`.
fn suggest_with_cli(
    binary: std::ffi::OsString,
    args: &[&str],
    stdin_input: Option<&str>,
    model: &str,
    name: &str,
) -> Result<ImagePromptSuggestResult, String> {
    use std::io::Write;

    let mut command = Command::new(binary);
    command
        .args(args)
        .stdin(if stdin_input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start {name}: {error}"))?;
    if let (Some(input), Some(mut stdin)) = (stdin_input, child.stdin.take()) {
        stdin
            .write_all(input.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    let result = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !result.status.success() {
        let detail = String::from_utf8_lossy(&result.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("{name} exited with {}", result.status)
        } else {
            trim_error(&detail)
        });
    }
    let text = String::from_utf8_lossy(&result.stdout);
    let (suggestions, keywords) = parse_suggestions(&text);
    if suggestions.is_empty() {
        return Err(format!("{name} returned no suggestions"));
    }
    Ok(ImagePromptSuggestResult {
        model: model.to_string(),
        suggestions,
        keywords,
    })
}

/// The text the model reads: the instructions, the picked settings, the reference images and the user's prompt.
fn suggest_input(prompt: &str, brief: &str, references: &[PathBuf]) -> String {
    let mut text = format!("{SUGGEST_INSTRUCTIONS}\n\n");
    if !brief.is_empty() {
        text.push_str(&format!("Settings:\n{brief}\n\n"));
    }
    if !references.is_empty() {
        text.push_str("Reference images:\n");
        for reference in references {
            text.push_str(&format!("- {}\n", reference.display()));
        }
        text.push('\n');
    }
    text.push_str(&format!("User prompt:\n{prompt}"));
    text
}

/// Prompts and keywords out of a model's answer: the JSON object that was asked for, or a bare array of prompts.
fn parse_suggestions(text: &str) -> (Vec<String>, Vec<SuggestedKeyword>) {
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
        if start < end {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text[start..=end]) {
                return read_suggestions(&value);
            }
        }
    }
    let prompts = match (text.find('['), text.rfind(']')) {
        (Some(start), Some(end)) if start < end => {
            serde_json::from_str::<Vec<String>>(&text[start..=end]).unwrap_or_default()
        }
        _ => Vec::new(),
    };
    (clean_prompts(prompts), Vec::new())
}

/// `prompts` (or `suggestions`) and `keywords` out of the model's object.
fn read_suggestions(value: &serde_json::Value) -> (Vec<String>, Vec<SuggestedKeyword>) {
    let prompts = value
        .get("prompts")
        .or_else(|| value.get("suggestions"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let keywords = value
        .get("keywords")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let category = item.get("category")?.as_str()?.trim().to_lowercase();
            let term = item.get("value")?.as_str()?.trim();
            (!category.is_empty() && !term.is_empty()).then(|| SuggestedKeyword {
                category,
                value: term.to_string(),
            })
        })
        .take(MAX_KEYWORDS)
        .collect();
    (clean_prompts(prompts), keywords)
}

fn clean_prompts(prompts: Vec<String>) -> Vec<String> {
    prompts
        .iter()
        .map(|prompt| prompt.trim().to_string())
        .filter(|prompt| !prompt.is_empty())
        .take(MAX_SUGGESTIONS)
        .collect()
}

/// Each candidate of a run gets its own direction, so the variants of one prompt do not come out alike.
const VARIATIONS: [&str; 4] = [
    "Take a distinct visual direction: a different composition, camera angle and lighting from the obvious first take.",
    "Render it in a different art style than a literal photograph: for example painterly, illustrated or graphic.",
    "Use an unexpected colour palette and a bolder, more graphic composition.",
    "Use a quieter, minimal composition with generous negative space and a different mood.",
];

fn variation_hint(variant: u8) -> &'static str {
    VARIATIONS[(usize::from(variant).max(1) - 1) % VARIATIONS.len()]
}

fn generate_one(
    script: &Path,
    prompt: &str,
    variant: u8,
    model: &str,
    backend: &str,
    output: &Path,
    references: &[PathBuf],
) -> Result<Vec<String>, String> {
    if CANCELLED.load(Ordering::SeqCst) {
        return Err("stopped".to_string());
    }
    let mut command = Command::new("node");
    command
        .arg(script)
        .args([
            "--json",
            "--quiet",
            "--no-retry",
            "--timeout",
            "900",
            "--backend",
            backend,
            "--model",
            model,
            "--output",
        ])
        .arg(output)
        .arg("--prompt")
        .arg(format!(
            "Generate exactly one image. {prompt}\n\n{}",
            variation_hint(variant)
        ));
    for reference in references {
        command.arg("--input-ref").arg(reference);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start image generator: {error}"))?;
    let pid = child.id();
    ACTIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(pid);
    let output = child.wait_with_output();
    ACTIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(|running| *running != pid);
    let result = output.map_err(|error| format!("image generator failed: {error}"))?;
    if CANCELLED.load(Ordering::SeqCst) {
        return Err("stopped".to_string());
    }
    if !result.status.success() {
        let detail = String::from_utf8_lossy(&result.stderr).trim().to_string();
        return Err(if detail.is_empty() {
            format!("image generator exited with {}", result.status)
        } else {
            trim_error(&detail)
        });
    }
    let json: serde_json::Value = serde_json::from_slice(&result.stdout)
        .map_err(|error| format!("image generator returned invalid JSON: {error}"))?;
    let paths = json
        .get("images")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|image| image.get("path").and_then(serde_json::Value::as_str))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Err("image generator returned no image".to_string());
    }
    Ok(paths)
}

fn imagen_script() -> Result<PathBuf, String> {
    let relative = Path::new(".codex/skills/codex-imagen/scripts/codex-imagen.mjs");
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("AGENTDECK_CODEX_IMAGEN_SCRIPT") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(dir) = RESOURCE_DIR.get() {
        candidates.push(dir.join("codex-imagen").join("codex-imagen.mjs"));
    }
    if let Ok(current) = std::env::current_dir() {
        candidates.extend(current.ancestors().map(|root| root.join(relative)));
    }
    if let Ok(exe) = std::env::current_exe() {
        candidates.extend(exe.ancestors().map(|root| root.join(relative)));
    }
    if let Some(home) = home_folder() {
        candidates.push(home.join(relative));
        candidates.push(home.join(".agents/skills/codex-imagen/scripts/codex-imagen.mjs"));
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            "codex-imagen skill was not found; set AGENTDECK_CODEX_IMAGEN_SCRIPT to its script path"
                .to_string()
        })
}

fn output_root(cwd: Option<&str>) -> Result<PathBuf, String> {
    let base = cwd
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| home_folder().map(|home| home.join(".agentdeck")))
        .ok_or("cannot find a project or home folder")?;
    let root = base.join(".agentdeck").join("generated-images");
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

fn validate_references(paths: &[String]) -> Result<Vec<PathBuf>, String> {
    if paths.len() > 6 {
        return Err("use at most 6 reference images".to_string());
    }
    paths
        .iter()
        .map(|path| {
            let resolved = PathBuf::from(path)
                .canonicalize()
                .map_err(|error| error.to_string())?;
            if !resolved.is_file() {
                return Err(format!("reference is not a file: {path}"));
            }
            Ok(resolved)
        })
        .collect()
}

fn home_folder() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn unique_run_name() -> String {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("{stamp:x}-{}", std::process::id())
}

fn safe_name(value: &str) -> String {
    let name = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    if name.is_empty() {
        "image".to_string()
    } else {
        name
    }
}

fn trim_error(value: &str) -> String {
    const MAX: usize = 800;
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= MAX {
        compact
    } else {
        format!("{}…", compact.chars().take(MAX - 1).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::{safe_name, trim_error};

    #[test]
    fn output_names_are_safe_and_stable() {
        assert_eq!(safe_name("gpt-image/2"), "gpt-image-2");
        assert_eq!(safe_name(""), "image");
    }

    #[test]
    fn generator_errors_are_compact() {
        assert_eq!(trim_error("a   b\n c"), "a b c");
        assert!(trim_error(&"x".repeat(900)).chars().count() <= 800);
    }
}
