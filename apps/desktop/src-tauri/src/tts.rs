// Read an answer aloud with Piper (an offline neural voice, https://github.com/rhasspy/piper) and its Vietnamese voice.
// Nothing is bundled: the first time the speaker button is used the program (about 22 MB) and the voice (28 to 63 MB) are
// downloaded into the app's data folder, with progress events. Each sentence is turned into a WAV by one run of Piper
// and handed to the page, which plays them one after another while the next is being made.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

const PIPER_RELEASE: &str = "https://github.com/rhasspy/piper/releases/download/2023.11.14-2";
const VOICE_HOST: &str = "https://huggingface.co/rhasspy/piper-voices/resolve/main/vi/vi_VN";
/// The Vietnamese voices Piper has: the id the page uses, the folder on the voice host and the file name without its extension.
/// `vais1000` is the best of them (medium); the other two are smaller and lower in quality.
const VOICES: [(&str, &str, &str); 3] = [
    ("vais1000", "vais1000/medium", "vi_VN-vais1000-medium"),
    ("25hours", "25hours_single/low", "vi_VN-25hours_single-low"),
    ("vivos", "vivos/x_low", "vi_VN-vivos-x_low"),
];
/// One sentence at a time: a longer text is cut by the page, this only stops a runaway call.
const MAX_CHARS: usize = 1200;

/// The Piper build for an OS and CPU, as the release names it.
pub fn piper_asset(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("piper_windows_amd64.zip"),
        ("macos", "aarch64") => Some("piper_macos_aarch64.tar.gz"),
        ("macos", "x86_64") => Some("piper_macos_x64.tar.gz"),
        ("linux", "x86_64") => Some("piper_linux_x86_64.tar.gz"),
        ("linux", "aarch64") => Some("piper_linux_aarch64.tar.gz"),
        _ => None,
    }
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|error| error.to_string())?.join("tts"))
}

fn exe_in(root: &Path) -> PathBuf {
    root.join("piper").join(if cfg!(windows) { "piper.exe" } else { "piper" })
}

/// The voice for an id from the page; an unknown or missing id is the default (the first).
fn voice_spec(id: Option<&str>) -> (&'static str, &'static str) {
    let (_, folder, stem) = VOICES.iter().find(|(name, _, _)| Some(*name) == id).unwrap_or(&VOICES[0]);
    (folder, stem)
}

fn voice_in(root: &Path, id: Option<&str>) -> PathBuf {
    root.join("voice").join(format!("{}.onnx", voice_spec(id).1))
}

fn is_installed(root: &Path, id: Option<&str>) -> bool {
    let voice = voice_in(root, id);
    exe_in(root).is_file() && voice.is_file() && voice.with_extension("onnx.json").is_file()
}

#[derive(Serialize)]
pub struct Status {
    installed: bool,
    supported: bool,
}

#[tauri::command]
pub fn tts_status(app: AppHandle, voice: Option<String>) -> Result<Status, String> {
    Ok(Status { installed: is_installed(&root(&app)?, voice.as_deref()), supported: piper_asset(std::env::consts::OS, std::env::consts::ARCH).is_some() })
}

#[derive(Serialize, Clone)]
struct Progress {
    stage: &'static str,
    done: u64,
    total: u64,
}

static INSTALLING: AtomicBool = AtomicBool::new(false);

async fn download(app: &AppHandle, client: &reqwest::Client, url: &str, dest: &Path, stage: &'static str) -> Result<(), String> {
    if let Some(folder) = dest.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    let response = client.get(url).send().await.map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        return Err(format!("download failed ({}): {url}", response.status()));
    }
    let total = response.content_length().unwrap_or(0);
    let part = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&part).await.map_err(|error| error.to_string())?;
    let mut stream = response.bytes_stream();
    let (mut done, mut last) = (0u64, std::time::Instant::now());
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await.map_err(|error| error.to_string())?;
        done += chunk.len() as u64;
        if last.elapsed() > std::time::Duration::from_millis(200) {
            last = std::time::Instant::now();
            let _ = app.emit("tts-progress", Progress { stage, done, total });
        }
    }
    tokio::io::AsyncWriteExt::flush(&mut file).await.map_err(|error| error.to_string())?;
    drop(file);
    let _ = app.emit("tts-progress", Progress { stage, done, total: total.max(done) });
    std::fs::rename(&part, dest).map_err(|error| error.to_string())
}

/// Downloads Piper and the Vietnamese voice (once). Progress comes as `tts-progress` events.
#[tauri::command]
pub async fn tts_install(app: AppHandle, voice: Option<String>) -> Result<(), String> {
    let root = root(&app)?;
    if is_installed(&root, voice.as_deref()) {
        return Ok(());
    }
    let asset = piper_asset(std::env::consts::OS, std::env::consts::ARCH).ok_or("no Piper build for this system")?;
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("already installing".into());
    }
    let result = async {
        let client = reqwest::Client::builder().user_agent("AgentDeck").timeout(std::time::Duration::from_secs(900)).build().map_err(|error| error.to_string())?;
        let archive = root.join(asset);
        if !exe_in(&root).is_file() {
            download(&app, &client, &format!("{PIPER_RELEASE}/{asset}"), &archive, "program").await?;
            // `tar` reads both the .zip (Windows 10+ ships it) and the .tar.gz.
            let extracted = tokio::process::Command::new("tar").arg("-xf").arg(&archive).arg("-C").arg(&root).output().await.map_err(|error| error.to_string())?;
            let _ = std::fs::remove_file(&archive);
            if !extracted.status.success() {
                return Err(format!("could not unpack Piper: {}", String::from_utf8_lossy(&extracted.stderr)));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(exe_in(&root), std::fs::Permissions::from_mode(0o755));
            }
        }
        let voice_path = voice_in(&root, voice.as_deref());
        let (folder, stem) = voice_spec(voice.as_deref());
        let base = format!("{VOICE_HOST}/{folder}/{stem}");
        if !voice_path.is_file() {
            download(&app, &client, &format!("{base}.onnx"), &voice_path, "voice").await?;
        }
        if !voice_path.with_extension("onnx.json").is_file() {
            download(&app, &client, &format!("{base}.onnx.json"), &voice_path.with_extension("onnx.json"), "voice").await?;
        }
        Ok(())
    }
    .await;
    INSTALLING.store(false, Ordering::SeqCst);
    result
}

/// Removes what `tts_install` downloaded.
#[tauri::command]
pub fn tts_remove(app: AppHandle) -> Result<(), String> {
    let root = root(&app)?;
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    }
    Ok(())
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// One sentence as a `data:audio/wav;base64,…` URL. `speed` 1.0 is the voice's own pace.
#[tauri::command]
pub async fn tts_speak(app: AppHandle, text: String, speed: f32, voice: Option<String>) -> Result<String, String> {
    use base64::Engine;
    let root = root(&app)?;
    if !is_installed(&root, voice.as_deref()) {
        return Err("the voice is not installed".into());
    }
    let text: String = text.chars().take(MAX_CHARS).collect::<String>().replace(['\r', '\n'], " ");
    if text.trim().is_empty() {
        return Err("nothing to read".into());
    }
    let length_scale = (1.0 / speed.clamp(0.5, 2.0)).to_string();
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Write;
        let wav = std::env::temp_dir().join(format!("agentdeck-tts-{}-{}.wav", std::process::id(), COUNTER.fetch_add(1, Ordering::SeqCst)));
        let exe = exe_in(&root);
        let mut command = std::process::Command::new(&exe);
        command
            .current_dir(exe.parent().unwrap_or(&root))
            .arg("--model")
            .arg(voice_in(&root, voice.as_deref()))
            .arg("--output_file")
            .arg(&wav)
            .args(["--length_scale", &length_scale])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).and_then(|_| stdin.write_all(b"\n")).map_err(|error| error.to_string())?;
        }
        let status = child.wait().map_err(|error| error.to_string())?;
        let bytes = std::fs::read(&wav).map_err(|error| error.to_string());
        let _ = std::fs::remove_file(&wav);
        if !status.success() {
            return Err("Piper failed".to_string());
        }
        Ok(format!("data:audio/wav;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes?)))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_system_has_a_piper_build() {
        assert_eq!(piper_asset("windows", "x86_64"), Some("piper_windows_amd64.zip"));
        assert_eq!(piper_asset("macos", "aarch64"), Some("piper_macos_aarch64.tar.gz"));
        assert_eq!(piper_asset("macos", "x86_64"), Some("piper_macos_x64.tar.gz"));
        assert_eq!(piper_asset("linux", "x86_64"), Some("piper_linux_x86_64.tar.gz"));
        assert_eq!(piper_asset("linux", "aarch64"), Some("piper_linux_aarch64.tar.gz"));
        assert_eq!(piper_asset("windows", "aarch64"), None);
    }

    #[test]
    fn an_unknown_voice_is_the_default_one() {
        assert_eq!(voice_spec(None), voice_spec(Some("vais1000")));
        assert_eq!(voice_spec(Some("nope")), voice_spec(None));
        assert_eq!(voice_spec(Some("vivos")).1, "vi_VN-vivos-x_low");
    }

    #[test]
    fn nothing_is_installed_in_an_empty_folder() {
        let folder = std::env::temp_dir().join(format!("agentdeck-tts-test-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        assert!(!is_installed(&folder, None));
        assert!(!is_installed(&folder, Some("vivos")));
        let _ = std::fs::remove_dir_all(&folder);
    }
}
