// Claude Mini — Tauri app entry point.
//
// Architecture note: every platform-specific decision lives in `window.rs`.
// This file should stay portable so adding Linux (v0.2) is a small additive
// change, not a refactor.

mod anthropic;
mod commands;
mod secrets;
mod sessions;
mod tray;
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

/// Remove the machine-local secret the uninstaller can't reach on its own.
///
/// Invoked as `claude-mini.exe --uninstall-cleanup` from the NSIS
/// PREUNINSTALL hook — see `installer-hooks.nsh` for the exact conditions
/// (only when the user ticked "delete application data", and never during an
/// update).
///
/// Session transcripts are deliberately NOT touched here: they live under the
/// app data directory, which the uninstaller already clears with `RmDir /r`
/// when that same checkbox is ticked. This handles only the API key, which
/// sits in Windows Credential Manager and so survives any amount of directory
/// deletion.
///
/// Failure is swallowed on purpose. An uninstall must not fail because
/// cleanup did, and `secrets::delete` already treats "no entry" as success —
/// so the only errors reaching here mean the credential store itself was
/// unreachable, which a user cannot act on mid-uninstall anyway.
pub fn uninstall_cleanup() {
    let _ = secrets::delete();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Enforce a single running instance. Must be the FIRST plugin
        // registered (Tauri requirement — it needs to win the race before
        // anything else does meaningful setup). Without this, launching
        // the app a second time while it's already running hidden-in-tray
        // (Start Menu, a taskbar pin, a desktop shortcut — any normal
        // "open the app" gesture) spawns a competing process with its own
        // window and its own tray icon, unaware of the first. That's what
        // caused "taskbar and tray icon at the same time": two different
        // processes, not one confused one. Now a second launch attempt
        // just restores the existing instance instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_window(app);
        }))
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
                    // Routed through tray::hide_window/show_window (not a
                    // raw w.hide()/w.show()) so the summon shortcut keeps
                    // the "window visible XOR tray icon present" invariant
                    // exactly like the titlebar close button does.
                    if let Some(w) = app.get_webview_window("main") {
                        if w.is_visible().unwrap_or(false) {
                            tray::hide_window(app);
                        } else {
                            tray::show_window(app);
                        }
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(tray::TrayState::default())
        .manage(commands::ChatState::default())
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
            commands::cancel_chat,
            commands::save_api_key,
            commands::has_api_key,
            commands::delete_api_key,
            commands::api_key_status,
            commands::list_models,
            commands::save_session,
            commands::list_sessions,
            commands::load_session,
            commands::delete_session,
            commands::delete_all_sessions,
            commands::export_transcript,
            commands::quit_app,
            commands::install_update,
            commands::set_close_to_tray,
            commands::handle_close,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Claude Mini");
}
