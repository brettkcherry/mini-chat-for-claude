// Claude Mini — Tauri app entry point.
//
// Architecture note: every platform-specific decision lives in `window.rs`.
// This file should stay portable so adding Linux (v0.2) is a small additive
// change, not a refactor.

mod anthropic;
mod commands;
mod secrets;
mod sessions;
mod window;

use tauri::Manager;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_window_state::StateFlags;

/// Summon/dismiss shortcut: Ctrl+Shift+Space.
/// Exists because the ✕ button hides the window — without this, hiding
/// was a dead end (Brett found out the hard way).
fn toggle_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Save/restore window position + size across sessions.
        // First launch: no state file → window.rs places explicitly.
        // Subsequent launches: plugin restores; we leave it alone.
        // VISIBLE is excluded: the plugin must never decide whether the
        // window shows — window.rs shows it unconditionally in setup.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if let Some(w) = app.get_webview_window("main") {
                        if w.is_visible().unwrap_or(false) {
                            let _ = w.hide();
                        } else {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            if let Some(main) = app.get_webview_window("main") {
                window::apply_platform_chrome(&app.handle(), &main);
            }
            // Register the global summon/dismiss shortcut. Failure is
            // non-fatal (e.g. another app owns it) — log and move on.
            if let Err(e) = app.global_shortcut().register(toggle_shortcut()) {
                eprintln!("[claude-mini] global shortcut registration failed: {e}");
            }
            // Check for updates in the background. Silent on any failure
            // (no release published yet, offline, endpoint placeholder).
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri_plugin_updater::UpdaterExt;
                let Ok(updater) = handle.updater() else { return };
                if let Ok(Some(update)) = updater.check().await {
                    use tauri::Emitter;
                    let _ = handle.emit(
                        "update-available",
                        serde_json::json!({ "version": update.version }),
                    );
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::send_chat,
            commands::save_api_key,
            commands::has_api_key,
            commands::delete_api_key,
            commands::api_key_status,
            commands::save_session,
            commands::list_sessions,
            commands::load_session,
            commands::delete_session,
            commands::export_chat,
            commands::quit_app,
            commands::install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Claude Mini");
}
