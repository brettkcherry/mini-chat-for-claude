// Tauri commands exposed to the JS frontend via `invoke()`.

use tauri::{AppHandle, Emitter};

use crate::anthropic::{stream_chat, Message};

/// Health check / IPC sanity test.
#[tauri::command]
pub fn ping(name: &str) -> String {
    format!("pong: {name}")
}

/// Stream a chat turn from Anthropic. Emits `chat-chunk` events to the
/// frontend for each delta and a final one with `stop: true`.
///
/// Key resolution order: OS credential store first, `ANTHROPIC_API_KEY`
/// env var as a dev-convenience fallback.
#[tauri::command]
pub async fn send_chat(
    app: AppHandle,
    model: String,
    messages: Vec<Message>,
    effort: Option<String>,
) -> Result<(), String> {
    let api_key = crate::secrets::load()
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .ok_or_else(|| {
            "No API key configured. Click the key button in the title bar to add one.".to_string()
        })?;

    let app_for_chunks = app.clone();
    stream_chat(api_key, model, messages, effort, move |chunk| {
        let _ = app_for_chunks.emit("chat-chunk", chunk);
    })
    .await
}

// ---------- Sessions ----------

#[tauri::command]
pub fn save_session(app: AppHandle, session: crate::sessions::Session) -> Result<(), String> {
    crate::sessions::save(&app, &session)
}

#[tauri::command]
pub fn list_sessions(app: AppHandle) -> Result<Vec<crate::sessions::SessionMeta>, String> {
    crate::sessions::list(&app)
}

#[tauri::command]
pub fn load_session(app: AppHandle, id: String) -> Result<crate::sessions::Session, String> {
    crate::sessions::load(&app, &id)
}

#[tauri::command]
pub fn delete_session(app: AppHandle, id: String) -> Result<(), String> {
    crate::sessions::delete(&app, &id)
}

/// Write the transcript to Documents/Claude Mini/, return the full path.
#[tauri::command]
pub fn export_chat(app: AppHandle, markdown: String, title: String) -> Result<String, String> {
    crate::sessions::export_markdown(&app, &markdown, &title)
}

/// Fully exit. The ✕ button only hides the window — without this command
/// a release build can only be killed via Task Manager.
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Download and apply the pending update, then restart. Called from the
/// update banner after the user opts in.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or("no update available")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}

/// Validate + store the API key in the OS credential store.
#[tauri::command]
pub fn save_api_key(key: String) -> Result<(), String> {
    crate::secrets::save(&key)
}

/// Is a key available (keychain or env fallback)?
#[tauri::command]
pub fn has_api_key() -> bool {
    crate::secrets::load().is_some() || std::env::var("ANTHROPIC_API_KEY").is_ok()
}

#[derive(serde::Serialize)]
pub struct KeyStatus {
    /// A key is saved in the OS credential store.
    pub stored: bool,
    /// Last 4 characters of the stored key, for identification.
    pub suffix: Option<String>,
    /// ANTHROPIC_API_KEY env var is present (dev fallback).
    pub env_fallback: bool,
}

/// Current key situation, for the settings card. Never exposes the full key.
#[tauri::command]
pub fn api_key_status() -> KeyStatus {
    let stored = crate::secrets::load();
    let suffix = stored
        .as_deref()
        .and_then(|k| k.get(k.len().saturating_sub(4)..))
        .map(str::to_string);
    KeyStatus {
        stored: stored.is_some(),
        suffix,
        env_fallback: std::env::var("ANTHROPIC_API_KEY").is_ok(),
    }
}

/// Remove the stored key (env fallback unaffected).
#[tauri::command]
pub fn delete_api_key() -> Result<(), String> {
    crate::secrets::delete()
}
