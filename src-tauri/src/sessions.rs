// Session persistence: each chat session is one JSON file under
// <app_data_dir>/sessions/<id>.json. Deliberately simple — no database,
// no index; the directory listing IS the index. Sessions are small text
// blobs, and file-per-session means the user can inspect, back up, or
// delete them by hand.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

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
        if let Some(meta) = meta_for_listing(s) {
            metas.push(meta);
        }
    }
    metas.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms));
    Ok(metas)
}

/// Turn a parsed session into its listing entry, or drop it.
///
/// The id travels straight into the sessions list as an HTML attribute, and
/// these files are user-editable by design (the README invites people to
/// inspect and back them up). `save` can only ever write a validated id, but
/// nothing stops a hand-edited or restored file from carrying something else —
/// so re-validate on the way out rather than trusting the file's contents.
/// Split out from `list` so this can be tested without an `AppHandle`.
fn meta_for_listing(s: Session) -> Option<SessionMeta> {
    if validate_session_id(&s.id).is_err() {
        return None;
    }
    Some(SessionMeta {
        message_count: s.messages.len(),
        id: s.id,
        title: s.title,
        updated_ms: s.updated_ms,
    })
}

pub fn load(app: &AppHandle, id: &str) -> Result<Session, String> {
    let path = session_path(app, id)?;
    let text = fs::read_to_string(&path).map_err(|e| format!("read failed: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse failed: {e}"))
}

/// Delete every saved session, returning how many files were removed.
///
/// Scoped to `*.json` directly inside the sessions directory: this is a
/// user-facing "delete all my chat history" action, so it must not become a
/// recursive wipe of anything else that happens to live nearby.
pub fn delete_all(app: &AppHandle) -> Result<usize, String> {
    delete_all_in(&sessions_dir(app)?)
}

/// The scanning half of `delete_all`, split out so the blast radius can be
/// tested against a real directory without an `AppHandle`.
fn delete_all_in(dir: &std::path::Path) -> Result<usize, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("read dir failed: {e}"))?;
    let mut removed = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => removed += 1,
            // Something else got there first — that's the outcome we wanted.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("delete failed for {}: {e}", path.display())),
        }
    }
    Ok(removed)
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

/// Filename the save dialog opens with: sanitized title + timestamp, so two
/// exports of the same chat never collide.
fn default_export_name(title: &str) -> String {
    let stem = sanitize_title_stem(title);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{stem}-{ts}.md")
}

/// Export a transcript. Returns the path written to, or `None` if the user
/// cancelled the dialog.
///
/// The dialog runs *here*, in Rust, and the only path ever written to is the
/// one it returns. That is the whole point of this function's shape. It used
/// to be two commands — one handing the frontend a suggested path, one taking
/// a path back and calling `fs::write` on it — which made the save dialog a
/// frontend convention rather than an enforced boundary. Anything with control
/// of the webview (a DOMPurify bypass on model output, say) could skip the
/// dialog and write anywhere the user could: the Startup folder, for one.
/// Now the frontend supplies only the *contents*; where they land is a
/// decision it cannot reach. `write` takes the same line in its
/// src-tauri/src/lib.rs, for the same reason.
///
/// Async so Tauri runs it off the main thread — a blocking dialog on the main
/// thread deadlocks the UI on Windows and macOS alike.
pub async fn export_transcript(
    app: &AppHandle,
    title: &str,
    markdown: &str,
) -> Result<Option<String>, String> {
    let dir = app
        .path()
        .document_dir()
        .map_err(|e| format!("no documents dir: {e}"))?
        .join("Mini Chat for Claude");
    // Created so the dialog has somewhere to open. Nothing is written unless
    // and until the user confirms a filename.
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create export dir: {e}"))?;

    let picked = app
        .dialog()
        .file()
        .set_title("Save chat as Markdown")
        .set_directory(&dir)
        .set_file_name(default_export_name(title))
        .add_filter("Markdown", &["md"])
        .blocking_save_file();

    let Some(picked) = picked else { return Ok(None) }; // cancelled

    let path = picked
        .into_path()
        .map_err(|e| format!("unusable path from dialog: {e}"))?;
    fs::write(&path, markdown).map_err(|e| format!("write failed: {e}"))?;
    Ok(Some(path.to_string_lossy().into_owned()))
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

    fn session_with_id(id: &str) -> Session {
        Session {
            id: id.to_string(),
            title: "Test".to_string(),
            created_ms: 1000,
            updated_ms: 2000,
            model: "claude-x".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: "hi".to_string(),
            }],
        }
    }

    #[test]
    fn meta_for_listing_keeps_valid_ids() {
        let meta = meta_for_listing(session_with_id("s1770000000000"))
            .expect("a normal id should be listed");
        assert_eq!(meta.id, "s1770000000000");
        assert_eq!(meta.message_count, 1);
    }

    /// A hand-edited session file must not be able to smuggle markup into the
    /// sessions list, where the id is interpolated into an HTML attribute.
    #[test]
    fn meta_for_listing_drops_ids_that_could_break_out_of_an_attribute() {
        for bad in [
            r#"s1" onclick="alert(1)"#,
            "s1'><script>alert(1)</script>",
            "../../etc/passwd",
            "s1 s2",
            "",
        ] {
            assert!(
                meta_for_listing(session_with_id(bad)).is_none(),
                "expected id {bad:?} to be dropped from the listing"
            );
        }
    }

    #[test]
    fn delete_all_in_removes_only_session_json() {
        let dir = std::env::temp_dir().join(format!(
            "claude-mini-delete-all-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        fs::write(dir.join("s1.json"), "{}").unwrap();
        fs::write(dir.join("s2.json"), "{}").unwrap();
        // Neither of these is chat history, so neither should be touched.
        fs::write(dir.join("notes.txt"), "keep me").unwrap();
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("nested").join("s3.json"), "{}").unwrap();

        let removed = delete_all_in(&dir).expect("delete_all_in should succeed");

        assert_eq!(removed, 2, "only the two top-level .json files count");
        assert!(!dir.join("s1.json").exists());
        assert!(!dir.join("s2.json").exists());
        assert!(dir.join("notes.txt").exists(), "non-json must survive");
        assert!(
            dir.join("nested").join("s3.json").exists(),
            "must not recurse into subdirectories"
        );

        // An already-empty directory is a no-op, not an error.
        fs::remove_file(dir.join("notes.txt")).unwrap();
        fs::remove_dir_all(dir.join("nested")).unwrap();
        assert_eq!(delete_all_in(&dir).unwrap(), 0);

        fs::remove_dir_all(&dir).ok();
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
