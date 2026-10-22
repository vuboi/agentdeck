use std::path::{Path, PathBuf};

mod browser;

use ag_adapters::agent::claude::Claude;
use ag_adapters::agent::runner::{drive_turn, resolve_binary};
use ag_adapters::agent::{AgentEvent, Provider, TurnRequest};
use ag_adapters::history::{self, HistoryMessage, HistorySession, Roots, Source};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

fn roots() -> Result<Roots, String> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .ok_or("cannot find the home folder")?;
    Ok(Roots::from_home(&home))
}

/// Saves a pasted or dropped image under `~/.agentdeck/attachments` so the agent CLI can open it by path.
/// `data` is the base64 of the file. Returns the path.
#[tauri::command]
async fn attachment_save(mime: String, data: String) -> Result<String, String> {
    use base64::Engine;
    const MAX_BYTES: usize = 15 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let extension = match mime.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            other => return Err(format!("{other} is not a supported image type")),
        };
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data.as_bytes())
            .map_err(|error| error.to_string())?;
        if bytes.len() > MAX_BYTES {
            return Err("the image is larger than 15 MB".to_string());
        }
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or("cannot find the home folder")?;
        let folder = home.join(".agentdeck").join("attachments");
        std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let path = folder.join(format!("image-{stamp:x}.{extension}"));
        std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Sessions Claude Code and Codex already saved on this machine. Read-only.
#[tauri::command]
async fn history_list() -> Result<Vec<HistorySession>, String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || history::list_all(&roots))
        .await
        .map_err(|error| error.to_string())
}

/// The transcript of one saved session. The path must be inside the history folders.
#[tauri::command]
async fn history_load(source: Source, path: String) -> Result<Vec<HistoryMessage>, String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || history::load(&roots, source, Path::new(&path)))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

/// Renames a saved session inside Claude's or Codex's own storage.
#[tauri::command]
async fn history_rename(source: Source, path: String, title: String) -> Result<(), String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || {
        history::rename(&roots, source, Path::new(&path), &title)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

/// Moves a saved session to `~/.agentdeck/trash` (not destroyed), so it leaves Claude and Codex too.
#[tauri::command]
async fn history_delete(source: Source, path: String) -> Result<(), String> {
    let roots = roots()?;
    tauri::async_runtime::spawn_blocking(move || {
        history::delete(&roots, source, Path::new(&path)).map(|_| ())
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

/// Text of one file for the file viewer. Read-only, text only, at most 2 MB, and the file must sit
/// inside `root` (the project folder), so a path from the UI cannot reach outside it.
#[tauri::command]
async fn file_read(root: String, path: String) -> Result<String, String> {
    const MAX_BYTES: u64 = 2 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("file is outside the project folder".to_string());
        }
        let size = std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len();
        if size > MAX_BYTES {
            return Err(format!("file is {size} bytes, the viewer stops at 2 MB"));
        }
        std::fs::read_to_string(&path).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Bytes of one file for previews (images, PDF, SVG), base64-encoded. Same rules as `file_read`:
/// read-only, inside `root`, and at most 20 MB.
#[tauri::command]
async fn file_read_bytes(root: String, path: String) -> Result<String, String> {
    use base64::Engine;
    const MAX_BYTES: u64 = 20 * 1024 * 1024;
    tauri::async_runtime::spawn_blocking(move || {
        let root = PathBuf::from(&root)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        let path = PathBuf::from(&path)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if !path.starts_with(&root) {
            return Err("file is outside the project folder".to_string());
        }
        let size = std::fs::metadata(&path)
            .map_err(|error| error.to_string())?
            .len();
        if size > MAX_BYTES {
            return Err(format!("file is {size} bytes, the preview stops at 20 MB"));
        }
        let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
        Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// One event of one chat, sent to the UI on the `agent-event` channel.
#[derive(Clone, Serialize)]
struct AgentEnvelope {
    session: String,
    event: AgentEvent,
}

fn provider_for(agent: &str) -> Result<Box<dyn Provider>, String> {
    match agent {
        "claude" => Ok(Box::new(Claude)),
        other => Err(format!("no agent provider for `{other}` yet")),
    }
}

/// Runs one turn for the app: resolves the binary, then forwards every event to the UI.
async fn run_turn(app: AppHandle, session: String, provider: Box<dyn Provider>, req: TurnRequest) {
    let binary = resolve_binary(provider.as_ref());
    drive_turn(binary, provider.as_ref(), &req, |event| {
        let _ = app.emit(
            "agent-event",
            AgentEnvelope {
                session: session.clone(),
                event,
            },
        );
    })
    .await;
}

/// Sends one user message to an agent CLI. Returns at once; the answer arrives as `agent-event`s.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // one parameter per field the frontend sends
async fn agent_send(
    app: AppHandle,
    session: String,
    agent: String,
    prompt: String,
    agent_session: Option<String>,
    resume: bool,
    model: Option<String>,
    permission_mode: Option<String>,
    allowed_tools: Vec<String>,
    cwd: Option<String>,
) -> Result<(), String> {
    let provider = provider_for(&agent)?;
    let req = TurnRequest {
        prompt,
        session_id: agent_session,
        resume,
        model,
        permission_mode,
        allowed_tools,
        cwd: cwd.map(Into::into),
    };
    tauri::async_runtime::spawn(run_turn(app, session, provider, req));
    Ok(())
}

/// Starts the AgentDeck desktop window.
pub fn run() {
    tauri::Builder::default()
        .register_uri_scheme_protocol("agentdeck", |context, request| {
            browser::handle_report(context.app_handle(), request.body());
            tauri::http::Response::builder()
                .status(200)
                .header("Access-Control-Allow-Origin", "*")
                .body(Vec::new())
                .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
        })
        .setup(|app| {
            if let Ok(url) = std::env::var("AGENTDECK_BROWSER_SELFTEST") {
                browser::selftest(app.handle().clone(), url);
            }
            // Developer hook: open the Preview tool on a page a few seconds after start-up.
            if let Ok(url) = std::env::var("AGENTDECK_OPEN_PREVIEW") {
                let handle = app.handle().clone();
                tauri::Listener::listen(app.handle(), "browser-report", |event| {
                    let shown: String = event.payload().chars().take(140).collect();
                    eprintln!("report: {shown}");
                });
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    let _ = tauri::Emitter::emit(&handle, "dev-preview", url);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            history_list,
            history_load,
            history_rename,
            history_delete,
            file_read,
            file_read_bytes,
            agent_send,
            attachment_save,
            browser::browser_show,
            browser::browser_bounds,
            browser::browser_hide,
            browser::browser_navigate,
            browser::browser_eval,
            browser::browser_zoom,
            browser::browser_close
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AgentDeck desktop application");
}
