// Session persistence: each chat session is one JSON file under
// <app_data_dir>/sessions/<id>.json. Deliberately simple — no database,
// no index; the directory listing IS the index. Sessions are small text
// blobs, and file-per-session means the user can inspect, back up, or
// delete them by hand.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::anthropic::Message;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub model: String,
    pub messages: Vec<Message>,
}

/// Listing entry — everything but the message bodies.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub updated_ms: u64,
    pub message_count: usize,
}

fn sessions_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?
        .join("sessions");
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create sessions dir: {e}"))?;
    Ok(dir)
}

/// IDs are generated in the frontend ("s" + epoch millis). Sanitize hard
/// anyway: an id is a filename, and filenames must never traverse.
fn session_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("invalid session id: {id:?}"));
    }
    Ok(sessions_dir(app)?.join(format!("{id}.json")))
}

pub fn save(app: &AppHandle, session: &Session) -> Result<(), String> {
    let path = session_path(app, &session.id)?;
    let json = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("write failed: {e}"))
}

pub fn list(app: &AppHandle) -> Result<Vec<SessionMeta>, String> {
    let dir = sessions_dir(app)?;
    let mut metas: Vec<SessionMeta> = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("read dir failed: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue; // unreadable file — skip, don't fail the whole list
        };
        let Ok(s) = serde_json::from_str::<Session>(&text) else {
            continue; // corrupt file — skip
        };
        metas.push(SessionMeta {
            id: s.id,
            title: s.title,
            updated_ms: s.updated_ms,
            message_count: s.messages.len(),
        });
    }
    metas.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms));
    Ok(metas)
}

pub fn load(app: &AppHandle, id: &str) -> Result<Session, String> {
    let path = session_path(app, id)?;
    let text = fs::read_to_string(&path).map_err(|e| format!("read failed: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse failed: {e}"))
}

pub fn delete(app: &AppHandle, id: &str) -> Result<(), String> {
    let path = session_path(app, id)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("delete failed: {e}")),
    }
}

/// Write an exported transcript to Documents/Claude Mini/. Returns the path.
pub fn export_markdown(app: &AppHandle, markdown: &str, title: &str) -> Result<String, String> {
    let dir = app
        .path()
        .document_dir()
        .map_err(|e| format!("no documents dir: {e}"))?
        .join("Mini Chat for Claude");
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create export dir: {e}"))?;

    // Sanitized title + timestamp → unique, readable filename.
    let safe: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .take(40)
        .collect();
    let safe = safe.trim().replace(' ', "-");
    let stem = if safe.is_empty() { "chat".to_string() } else { safe };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("{stem}-{ts}.md"));

    fs::write(&path, markdown).map_err(|e| format!("write failed: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}
