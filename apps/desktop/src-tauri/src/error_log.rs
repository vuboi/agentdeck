//! The error log: one JSON line per problem the app met while it worked (a turn that failed, a step or a team member
//! that stopped, a hand-back that never ran, an agent quiet for too long, an uncaught error in the page). Kept on disk,
//! so a hang or two agents that could not reach each other can be traced after the fact. Part of the Audit log: the
//! Tasks view shows it in a drawer and saves it where the user picks.

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use tauri::{AppHandle, Manager};

/// Past this size the log is moved to `errors.1.log` (one older file kept) and a new one starts.
const ROTATE_BYTES: u64 = 2 * 1024 * 1024;
const PAGE_SIZE_MAX: usize = 100;

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum LogFile {
    Current,
    Rotated,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorLogCursor {
    file: LogFile,
    offset: u64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorLogPageEntry {
    file: LogFile,
    offset: u64,
    line: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorLogPage {
    entries: Vec<ErrorLogPageEntry>,
    cursor: Option<ErrorLogCursor>,
    has_more: bool,
    count: Option<usize>,
    areas: Option<Vec<String>>,
}

fn log_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("logs");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

fn current(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(log_dir(app)?.join("errors.log"))
}

/// Appends lines (JSON objects, one per entry) to the log.
#[tauri::command]
pub async fn error_log_append(app: AppHandle, lines: Vec<String>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = current(&app)?;
        if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > ROTATE_BYTES) {
            let _ = std::fs::rename(&path, path.with_file_name("errors.1.log"));
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| error.to_string())?;
        for line in lines {
            // One entry per line, whatever the text holds.
            let line = line.replace(['\r', '\n'], " ");
            writeln!(file, "{line}").map_err(|error| error.to_string())?;
        }
        Ok(())
    })
    .await
    .map_err(|error| error.to_string())?
}

fn previous_newline(file: &mut std::fs::File, before: u64) -> std::io::Result<Option<u64>> {
    const CHUNK: u64 = 8192;
    let mut end = before;
    while end > 0 {
        let start = end.saturating_sub(CHUNK);
        let mut bytes = vec![0; (end - start) as usize];
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut bytes)?;
        if let Some(index) = bytes.iter().rposition(|byte| *byte == b'\n') {
            return Ok(Some(start + index as u64));
        }
        end = start;
    }
    Ok(None)
}

fn summarize(paths: &[PathBuf]) -> Result<(usize, Vec<String>), String> {
    let mut count = 0;
    let mut areas = std::collections::BTreeSet::new();
    for path in paths {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.to_string()),
        };
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|error| error.to_string())?;
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                count += 1;
                if let Some(area) = value.get("area").and_then(serde_json::Value::as_str) {
                    areas.insert(area.to_string());
                }
            }
        }
    }
    Ok((count, areas.into_iter().collect()))
}

/// Reads a bounded page, newest first on disk and chronological within the page. The cursor points at the oldest
/// returned line; new appends do not invalidate it. The current and one rotated file form the retained history.
#[tauri::command]
pub async fn error_log_read(
    app: AppHandle,
    cursor: Option<ErrorLogCursor>,
    limit: usize,
) -> Result<ErrorLogPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let current_path = current(&app)?;
        let rotated_path = current_path.with_file_name("errors.1.log");
        let (count, areas) = if cursor.is_none() {
            let (count, areas) = summarize(&[rotated_path.clone(), current_path.clone()])?;
            (Some(count), Some(areas))
        } else {
            (None, None)
        };
        let page_size = limit.clamp(1, PAGE_SIZE_MAX);
        let mut location = cursor.unwrap_or_else(|| ErrorLogCursor {
            file: LogFile::Current,
            offset: std::fs::metadata(&current_path).map_or(0, |meta| meta.len()),
        });

        let mut entries = Vec::with_capacity(page_size + 1);
        let mut page_cursor = None;
        while entries.len() <= page_size {
            let path = match location.file {
                LogFile::Current => &current_path,
                LogFile::Rotated => &rotated_path,
            };
            let mut file = match std::fs::File::open(path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if matches!(location.file, LogFile::Current) {
                        location = ErrorLogCursor {
                            file: LogFile::Rotated,
                            offset: std::fs::metadata(&rotated_path).map_or(0, |meta| meta.len()),
                        };
                        continue;
                    }
                    break;
                }
                Err(error) => return Err(error.to_string()),
            };
            let file_len = file.metadata().map_err(|error| error.to_string())?.len();
            if location.offset > file_len {
                // If the current file rotated between pages, its former bytes now start in errors.1.log.
                if matches!(location.file, LogFile::Current) && rotated_path.is_file() {
                    location = ErrorLogCursor {
                        file: LogFile::Rotated,
                        offset: location
                            .offset
                            .min(std::fs::metadata(&rotated_path).map_or(0, |meta| meta.len())),
                    };
                    continue;
                }
                break;
            }
            let mut end = location.offset.min(file_len);
            if end == 0 {
                if matches!(location.file, LogFile::Current) {
                    location = ErrorLogCursor {
                        file: LogFile::Rotated,
                        offset: std::fs::metadata(&rotated_path).map_or(0, |meta| meta.len()),
                    };
                    continue;
                }
                break;
            }
            file.seek(SeekFrom::Start(end - 1))
                .map_err(|error| error.to_string())?;
            let mut last = [0];
            file.read_exact(&mut last)
                .map_err(|error| error.to_string())?;
            if last[0] == b'\n' {
                end -= 1;
            }
            let separator = previous_newline(&mut file, end).map_err(|error| error.to_string())?;
            let start = separator.map_or(0, |offset| offset + 1);
            if start < end {
                let mut bytes = vec![0; (end - start) as usize];
                file.seek(SeekFrom::Start(start))
                    .map_err(|error| error.to_string())?;
                file.read_exact(&mut bytes)
                    .map_err(|error| error.to_string())?;
                if !bytes.iter().all(u8::is_ascii_whitespace)
                    && serde_json::from_slice::<serde_json::Value>(&bytes).is_ok()
                {
                    entries.push(ErrorLogPageEntry {
                        file: location.file,
                        offset: start,
                        line: String::from_utf8_lossy(&bytes).into_owned(),
                    });
                    if entries.len() <= page_size {
                        page_cursor = Some(ErrorLogCursor {
                            file: location.file,
                            offset: start,
                        });
                    }
                }
            }
            location.offset = separator.unwrap_or(0);
        }
        let has_more = entries.len() > page_size;
        entries.truncate(page_size);
        entries.reverse();
        Ok(ErrorLogPage {
            entries,
            cursor: has_more.then_some(page_cursor).flatten(),
            has_more,
            count,
            areas,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Saves the whole log (older file included) where the user picks. `None` when the dialog was cancelled.
#[tauri::command]
pub async fn error_log_save(app: AppHandle, name: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let path = current(&app)?;
        let mut text =
            std::fs::read_to_string(path.with_file_name("errors.1.log")).unwrap_or_default();
        text.push_str(&std::fs::read_to_string(&path).unwrap_or_default());
        let Some(destination) = app
            .dialog()
            .file()
            .set_title("Save the AgentDeck error log")
            .set_file_name(name)
            .add_filter("Log", &["log", "txt", "jsonl"])
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let destination = destination.into_path().map_err(|error| error.to_string())?;
        std::fs::write(&destination, text).map_err(|error| error.to_string())?;
        Ok(Some(destination.display().to_string()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Empties the log (both files).
#[tauri::command]
pub async fn error_log_clear(app: AppHandle) -> Result<(), String> {
    let path = current(&app)?;
    let _ = std::fs::remove_file(path.with_file_name("errors.1.log"));
    std::fs::write(&path, "").map_err(|error| error.to_string())
}
