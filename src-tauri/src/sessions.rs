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
fn validate_session_id(id: &str) -> Result<(), String> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(format!("invalid session id: {id:?}"));
    }
    Ok(())
}

fn session_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    validate_session_id(id)?;
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

/// Sanitize a title into a filesystem-safe filename stem: keep
/// alphanumeric/space/`-`, replace everything else with `_`, cap at 40
/// chars, trim, and turn spaces into dashes. Falls back to "chat" if that
/// leaves nothing.
fn sanitize_title_stem(title: &str) -> String {
    let safe: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .take(40)
        .collect();
    let safe = safe.trim().replace(' ', "-");
    if safe.is_empty() { "chat".to_string() } else { safe }
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
    let stem = sanitize_title_stem(title);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("{stem}-{ts}.md"));

    fs::write(&path, markdown).map_err(|e| format!("write failed: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anthropic::Message;

    #[test]
    fn validate_session_id_accepts_alphanumeric() {
        assert!(validate_session_id("s123").is_ok());
        assert!(validate_session_id("abc").is_ok());
    }

    #[test]
    fn validate_session_id_rejects_empty() {
        assert!(validate_session_id("").is_err());
    }

    #[test]
    fn validate_session_id_rejects_traversal_and_special_chars() {
        for bad in ["../x", "a.b", "a/b", "a\\b", "a b", "a-b", "café"] {
            assert!(
                validate_session_id(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    #[test]
    fn sanitize_title_stem_normal() {
        assert_eq!(sanitize_title_stem("My Chat"), "My-Chat");
    }

    #[test]
    fn sanitize_title_stem_special_chars() {
        assert_eq!(sanitize_title_stem("a/b:c?d"), "a_b_c_d");
    }

    #[test]
    fn sanitize_title_stem_truncates_long_titles() {
        let long = "x".repeat(100);
        let stem = sanitize_title_stem(&long);
        assert!(stem.chars().count() <= 40);
    }

    #[test]
    fn sanitize_title_stem_empty_and_whitespace_fall_back_to_chat() {
        assert_eq!(sanitize_title_stem(""), "chat");
        assert_eq!(sanitize_title_stem("   "), "chat");
    }

    #[test]
    fn sanitize_title_stem_trims_and_dashes_spaces() {
        assert_eq!(sanitize_title_stem("  hello world  "), "hello-world");
    }

    #[test]
    fn session_serde_roundtrip_uses_camel_case() {
        let session = Session {
            id: "s1".to_string(),
            title: "Test".to_string(),
            created_ms: 1000,
            updated_ms: 2000,
            model: "claude-x".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
        };
        let json = serde_json::to_value(&session).unwrap();
        assert!(json.get("createdMs").is_some());
        assert!(json.get("updatedMs").is_some());
        assert!(json.get("created_ms").is_none());
        assert!(json.get("updated_ms").is_none());

        let round: Session = serde_json::from_value(json).unwrap();
        assert_eq!(round.id, session.id);
        assert_eq!(round.title, session.title);
        assert_eq!(round.created_ms, session.created_ms);
        assert_eq!(round.updated_ms, session.updated_ms);
        assert_eq!(round.model, session.model);
        assert_eq!(round.messages.len(), session.messages.len());
        assert_eq!(round.messages[0].role, session.messages[0].role);
        assert_eq!(round.messages[0].content, session.messages[0].content);
    }

    #[test]
    fn session_meta_serializes_message_count_camel_case() {
        let meta = SessionMeta {
            id: "s1".to_string(),
            title: "Test".to_string(),
            updated_ms: 2000,
            message_count: 3,
        };
        let json = serde_json::to_value(&meta).unwrap();
        assert!(json.get("messageCount").is_some());
        assert!(json.get("message_count").is_none());
        assert!(json.get("updatedMs").is_some());
    }
}
