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
/// Key resolution order: OS credential store first, then `ANTHROPIC_API_KEY`
/// — but only in debug builds. See `secrets::env_fallback`.
#[tauri::command]
pub async fn send_chat(
    app: AppHandle,
    model: String,
    messages: Vec<Message>,
    effort: Option<String>,
) -> Result<(), String> {
    let api_key = crate::secrets::load()
        .or_else(crate::secrets::env_fallback)
        .ok_or_else(|| {
            "No API key configured. Click the key button in the title bar to add one.".to_string()
        })?;

    let app_for_chunks = app.clone();
    stream_chat(api_key, model, messages, effort, move |chunk| {
        let _ = app_for_chunks.emit("chat-chunk", chunk);
    })
    .await
}

/// The account's available models, newest first, with per-model effort
/// capabilities. The frontend calls this at startup and falls back to its
/// own baked-in list if it fails (offline, no key yet, etc.).
#[tauri::command]
pub async fn list_models() -> Result<Vec<crate::anthropic::ModelInfo>, String> {
    let api_key = crate::secrets::load()
        .or_else(crate::secrets::env_fallback)
        .ok_or("No API key configured.")?;
    crate::anthropic::list_models(api_key).await
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

/// Erase all saved chat history, returning how many sessions were removed.
/// Irreversible — the frontend gates this behind an explicit confirm step.
#[tauri::command]
pub fn delete_all_sessions(app: AppHandle) -> Result<usize, String> {
    crate::sessions::delete_all(&app)
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

/// Persist the "Close to tray" Settings toggle. Called once at startup to
/// sync Rust with the saved preference, and again whenever the user
/// flips the toggle.
#[tauri::command]
pub fn set_close_to_tray(app: AppHandle, enabled: bool) {
    crate::tray::set_enabled(&app, enabled);
}

/// Titlebar ✕ click. Hides to tray if "close to tray" is on (default),
/// otherwise fully quits — see tray::handle_close_button for the reasoning.
#[tauri::command]
pub fn handle_close(app: AppHandle) {
    crate::tray::handle_close_button(&app);
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

/// Is a key available (keychain, or the debug-only env fallback)?
#[tauri::command]
pub fn has_api_key() -> bool {
    crate::secrets::load().is_some() || crate::secrets::env_fallback().is_some()
}

#[derive(serde::Serialize)]
pub struct KeyStatus {
    /// A key is saved in the OS credential store.
    pub stored: bool,
    /// Last 4 characters of the stored key, for identification.
    pub suffix: Option<String>,
    /// ANTHROPIC_API_KEY is being used. Always false in release builds — the
    /// fallback is compiled out there, so the UI never advertises it.
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
        env_fallback: crate::secrets::env_fallback().is_some(),
    }
}

/// Remove the stored key (env fallback unaffected).
#[tauri::command]
pub fn delete_api_key() -> Result<(), String> {
    crate::secrets::delete()
}
