//! Local history of image runs: the prompt, the models, and every candidate file, kept in SQLite so the list
//! survives a restart and can be synced later. Rows are only added, never rewritten: `id` names a run or a
//! file for good, and `created_at` plus the sha256 of each file are what a sync compares.

use std::path::Path;

use rusqlite::{params, Connection};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::image_gen::ImageGenerateResult;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS image_runs (
    id TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    prompt TEXT NOT NULL,
    models TEXT NOT NULL,
    variants INTEGER NOT NULL,
    status TEXT NOT NULL,
    failures TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS image_files (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES image_runs (id),
    model TEXT NOT NULL,
    backend TEXT NOT NULL,
    variant INTEGER NOT NULL,
    path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    bytes INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS image_files_run ON image_files (run_id);
";

#[derive(Debug, Serialize)]
pub struct HistoryImage {
    pub id: String,
    pub model: String,
    pub backend: String,
    pub variant: u8,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct HistoryRun {
    pub id: String,
    pub created_at: i64,
    pub prompt: String,
    pub models: Vec<String>,
    pub variants: u8,
    /// `done`, `stopped` (the user stopped it) or `failed` (no image came out).
    pub status: String,
    /// One line per candidate that failed: `model #variant: error`.
    pub failures: Vec<String>,
    pub images: Vec<HistoryImage>,
}

fn open() -> Result<Connection, String> {
    let folder = crate::home_folder()?.join(".agentdeck");
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let conn =
        Connection::open(folder.join("image-history.db")).map_err(|error| error.to_string())?;
    conn.execute_batch(SCHEMA)
        .map_err(|error| error.to_string())?;
    Ok(conn)
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

/// The file's sha256 and size. A file that cannot be read is recorded with an empty hash.
fn fingerprint(path: &Path) -> (String, u64) {
    match std::fs::read(path) {
        Ok(bytes) => {
            let digest: String = Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            (digest, bytes.len() as u64)
        }
        Err(_) => (String::new(), 0),
    }
}

/// Saves one run and its candidate files in a single transaction.
pub fn record(
    run_id: &str,
    prompt: &str,
    models: &[String],
    variants: u8,
    status: &str,
    result: &ImageGenerateResult,
) -> Result<(), String> {
    let mut conn = open()?;
    let tx = conn.transaction().map_err(|error| error.to_string())?;
    let failures: Vec<String> = result
        .failures
        .iter()
        .map(|failure| format!("{} #{}: {}", failure.model, failure.variant, failure.error))
        .collect();
    tx.execute(
        "INSERT INTO image_runs (id, created_at, prompt, models, variants, status, failures) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            run_id,
            now_millis(),
            prompt,
            serde_json::to_string(models).unwrap_or_default(),
            variants,
            status,
            serde_json::to_string(&failures).unwrap_or_default(),
        ],
    )
    .map_err(|error| error.to_string())?;
    for (index, image) in result.images.iter().enumerate() {
        let (sha256, bytes) = fingerprint(Path::new(&image.path));
        tx.execute(
            "INSERT INTO image_files (id, run_id, model, backend, variant, path, sha256, bytes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![format!("{run_id}-{index}"), run_id, image.model, image.backend, image.variant, image.path, sha256, bytes as i64],
        )
        .map_err(|error| error.to_string())?;
    }
    tx.commit().map_err(|error| error.to_string())
}

/// The newest runs first, each with its files.
pub fn list(limit: usize) -> Result<Vec<HistoryRun>, String> {
    let conn = open()?;
    let mut runs_query = conn
        .prepare("SELECT id, created_at, prompt, models, variants, status, failures FROM image_runs ORDER BY created_at DESC LIMIT ?1")
        .map_err(|error| error.to_string())?;
    let rows = runs_query
        .query_map(params![limit as i64], |row| {
            let models: String = row.get(3)?;
            let failures: String = row.get(6)?;
            Ok(HistoryRun {
                id: row.get(0)?,
                created_at: row.get(1)?,
                prompt: row.get(2)?,
                models: serde_json::from_str(&models).unwrap_or_default(),
                variants: row.get(4)?,
                status: row.get(5)?,
                failures: serde_json::from_str(&failures).unwrap_or_default(),
                images: Vec::new(),
            })
        })
        .map_err(|error| error.to_string())?;
    let mut runs = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut files_query = conn
        .prepare("SELECT id, model, backend, variant, path, sha256, bytes FROM image_files WHERE run_id = ?1 ORDER BY variant, model")
        .map_err(|error| error.to_string())?;
    for run in &mut runs {
        let files = files_query
            .query_map(params![run.id], |row| {
                Ok(HistoryImage {
                    id: row.get(0)?,
                    model: row.get(1)?,
                    backend: row.get(2)?,
                    variant: row.get(3)?,
                    path: row.get(4)?,
                    sha256: row.get(5)?,
                    bytes: row.get::<_, i64>(6)? as u64,
                })
            })
            .map_err(|error| error.to_string())?;
        run.images = files
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
    }
    Ok(runs)
}

#[tauri::command]
pub fn image_history_list(limit: Option<u32>) -> Result<Vec<HistoryRun>, String> {
    list(limit.unwrap_or(50).clamp(1, 200) as usize)
}
