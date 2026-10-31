//! Offline speech transcription for the chat composer.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use base64::Engine;
use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const WHISPER_SAMPLE_RATE: u32 = 16_000;
const MAX_AUDIO_SECONDS: usize = 5 * 60;
const MIN_MODEL_BYTES: u64 = 100 * 1024 * 1024;
const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin?download=true";

type VoiceResult<T> = Result<T, VoiceError>;

#[derive(Debug, thiserror::Error)]
enum VoiceError {
    #[error("invalid recording: {0}")]
    InvalidAudio(&'static str),
    #[error("could not decode the recording: {0}")]
    Decode(#[from] base64::DecodeError),
    #[error("could not access speech files: {0}")]
    Io(#[from] std::io::Error),
    #[error("speech model download failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("speech transcription failed: {0}")]
    Whisper(#[from] whisper_rs::WhisperError),
    #[error("speech task failed: {0}")]
    Task(String),
}

#[derive(Clone, Serialize)]
struct ModelProgress {
    done: u64,
    total: u64,
}

fn model_path(app: &AppHandle) -> VoiceResult<PathBuf> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| VoiceError::Task(error.to_string()))?
        .join("voice")
        .join("ggml-base.bin"))
}

fn usable_model(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.len() >= MIN_MODEL_BYTES)
}

async fn download_model(app: &AppHandle, path: &Path) -> VoiceResult<()> {
    if let Some(folder) = path.parent() {
        tokio::fs::create_dir_all(folder).await?;
    }
    let client = reqwest::Client::builder()
        .user_agent("AgentDeck")
        .timeout(std::time::Duration::from_secs(1_800))
        .build()?;
    let response = client.get(MODEL_URL).send().await?.error_for_status()?;
    let total = response.content_length().unwrap_or(0);
    let part = path.with_extension("part");
    let mut file = tokio::fs::File::create(&part).await?;
    let mut stream = response.bytes_stream();
    let mut done = 0_u64;
    let mut last = std::time::Instant::now();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk).await?;
        done += chunk.len() as u64;
        if last.elapsed() >= std::time::Duration::from_millis(200) {
            last = std::time::Instant::now();
            let _ = app.emit("voice-model-progress", ModelProgress { done, total });
        }
    }
    tokio::io::AsyncWriteExt::flush(&mut file).await?;
    drop(file);
    let _ = app.emit(
        "voice-model-progress",
        ModelProgress {
            done,
            total: total.max(done),
        },
    );
    if done < MIN_MODEL_BYTES {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(VoiceError::Task(
            "the downloaded speech model is incomplete".into(),
        ));
    }
    tokio::fs::rename(&part, path).await?;
    Ok(())
}

static MODEL_DOWNLOAD: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

async fn ensure_model(app: &AppHandle) -> VoiceResult<PathBuf> {
    let path = model_path(app)?;
    if usable_model(&path) {
        return Ok(path);
    }
    let guard = MODEL_DOWNLOAD
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    if !usable_model(&path) {
        download_model(app, &path).await?;
    }
    drop(guard);
    Ok(path)
}

fn decode_pcm(encoded: &str) -> VoiceResult<Vec<f32>> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
    if bytes.is_empty() || bytes.len() % std::mem::size_of::<f32>() != 0 {
        return Err(VoiceError::InvalidAudio("PCM bytes are incomplete"));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

/// Downmixes interleaved floating-point PCM and resamples it for Whisper.
fn convert_pcm(samples: &[f32], sample_rate: u32, channels: usize) -> VoiceResult<Vec<f32>> {
    if sample_rate == 0 || channels == 0 {
        return Err(VoiceError::InvalidAudio(
            "sample rate and channels must be positive",
        ));
    }
    if samples.is_empty() || samples.len() % channels != 0 {
        return Err(VoiceError::InvalidAudio("channel data is incomplete"));
    }
    let frames = samples.len() / channels;
    if frames > sample_rate as usize * MAX_AUDIO_SECONDS {
        return Err(VoiceError::InvalidAudio(
            "recordings are limited to five minutes",
        ));
    }
    let mono = samples
        .chunks_exact(channels)
        .map(|frame| {
            let sum = frame
                .iter()
                .copied()
                .filter(|sample| sample.is_finite())
                .sum::<f32>();
            (sum / channels as f32).clamp(-1.0, 1.0)
        })
        .collect::<Vec<_>>();
    if sample_rate == WHISPER_SAMPLE_RATE {
        return Ok(mono);
    }

    let target_len = mono.len().saturating_mul(WHISPER_SAMPLE_RATE as usize) / sample_rate as usize;
    if target_len == 0 {
        return Err(VoiceError::InvalidAudio("recording is too short"));
    }
    let ratio = sample_rate as f64 / WHISPER_SAMPLE_RATE as f64;
    let mut converted = Vec::with_capacity(target_len);
    for target in 0..target_len {
        let position = target as f64 * ratio;
        let left = (position.floor() as usize).min(mono.len() - 1);
        let right = (left + 1).min(mono.len() - 1);
        let mix = (position - left as f64) as f32;
        converted.push(mono[left] + (mono[right] - mono[left]) * mix);
    }
    Ok(converted)
}

static CONTEXT: OnceLock<Mutex<Option<WhisperContext>>> = OnceLock::new();

fn transcribe(model: &Path, audio: &[f32]) -> VoiceResult<String> {
    let mut context = CONTEXT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| VoiceError::Task("speech model lock is unavailable".into()))?;
    if context.is_none() {
        *context = Some(WhisperContext::new_with_params(
            model,
            WhisperContextParameters::default(),
        )?);
    }
    let whisper = context
        .as_ref()
        .ok_or_else(|| VoiceError::Task("speech model did not load".into()))?;
    let mut state = whisper.create_state()?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("vi"));
    params.set_translate(false);
    params.set_no_context(true);
    params.set_no_timestamps(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_n_threads(
        std::thread::available_parallelism().map_or(4, |threads| threads.get().min(8)) as i32,
    );
    state.full(params, audio)?;
    Ok(state
        .as_iter()
        .map(|segment| segment.to_string())
        .collect::<String>()
        .trim()
        .to_string())
}

/// Transcribes a browser recording locally with the multilingual Whisper base model.
#[tauri::command]
pub async fn voice_transcribe(
    app: AppHandle,
    audio_base64: String,
    sample_rate: u32,
    channels: usize,
) -> Result<String, String> {
    let result = async {
        let model = ensure_model(&app).await?;
        let samples = decode_pcm(&audio_base64)?;
        let audio = convert_pcm(&samples, sample_rate, channels)?;
        tauri::async_runtime::spawn_blocking(move || transcribe(&model, &audio))
            .await
            .map_err(|error| VoiceError::Task(error.to_string()))?
    }
    .await;
    result.map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_conversion_downmixes_stereo() {
        let converted = convert_pcm(&[1.0, -1.0, 0.5, 0.5], WHISPER_SAMPLE_RATE, 2)
            .expect("stereo PCM should convert");
        assert_eq!(converted, vec![0.0, 0.5]);
    }

    #[test]
    fn pcm_conversion_resamples_to_sixteen_kilohertz() {
        let source = (0..48_000)
            .map(|sample| sample as f32 / 48_000.0)
            .collect::<Vec<_>>();
        let converted = convert_pcm(&source, 48_000, 1).expect("48 kHz PCM should convert");
        assert_eq!(converted.len(), 16_000);
        assert_eq!(converted[0], 0.0);
        assert_eq!(converted[1], 3.0 / 48_000.0);
        assert_eq!(converted[15_999], 47_997.0 / 48_000.0);
    }

    #[test]
    fn pcm_conversion_rejects_invalid_metadata() {
        assert!(convert_pcm(&[0.0], 0, 1).is_err());
        assert!(convert_pcm(&[0.0], 16_000, 0).is_err());
        assert!(convert_pcm(&[0.0, 1.0, 2.0], 16_000, 2).is_err());
    }
}
