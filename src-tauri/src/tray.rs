// System tray icon — shown only while the window is hidden AND the user
// has enabled "Close to tray" in Settings.
//
// By design the two states are mutually exclusive: window visible XOR
// tray icon present, never both, never neither once the app has started.
// That invariant is enforced structurally, not by convention — every
// hide/show in the app (the titlebar close button, the global summon
// shortcut) routes through hide_window()/show_window() below rather than
// calling window.hide()/show() directly, so there's exactly one place
// that creates the icon and exactly one place that destroys it.

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{image::Image, AppHandle, Manager};

const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/32x32.png");
const QUIT_ITEM_ID: &str = "tray-quit";

pub struct TrayState {
    enabled: Mutex<bool>,
    icon: Mutex<Option<TrayIcon>>,
}

impl Default for TrayState {
    fn default() -> Self {
        // "Close to tray" defaults ON: the titlebar ✕ minimizes to tray
        // unless the user explicitly opts out in Settings. A hidden app
        // with zero visible affordance anywhere (no taskbar, no tray) and
        // recoverable only via a memorized keyboard shortcut is a
        // discoverability trap for anyone who doesn't already know it —
        // see PLAN.md for the UX discussion this came out of.
        Self {
            enabled: Mutex::new(true),
            icon: Mutex::new(None),
        }
    }
}

pub fn set_enabled(app: &AppHandle, enabled: bool) {
    let state = app.state::<TrayState>();
    *state.enabled.lock().unwrap() = enabled;
    if !enabled {
        // Don't leave a stale icon around if the user disables this
        // while one happens to be showing.
        *state.icon.lock().unwrap() = None;
    }
}

/// What the titlebar ✕ button means: hide-to-tray if "close to tray" is
/// on (the default), otherwise a full quit — closing behaves exactly
/// like any other Windows app's ✕ when the user has opted out of tray
/// mode, with nothing left running and no shortcut-summon possible.
///
/// The global summon shortcut deliberately does NOT route through this
/// function — it always just hides (see lib.rs), since its whole point
/// is "dismiss, still recoverable via the same shortcut." Quitting the
/// app from a dismiss keypress would be a surprising, destructive way
/// for that shortcut to behave.
pub fn handle_close_button(app: &AppHandle) {
    let state = app.state::<TrayState>();
    let enabled = *state.enabled.lock().unwrap();
    if enabled {
        hide_window(app);
    } else {
        app.exit(0);
    }
}

/// Hide the window. If "close to tray" is on, also raise a tray icon
/// (created once, reused on subsequent hides) that restores the window.
pub fn hide_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }

    let state = app.state::<TrayState>();
    if !*state.enabled.lock().unwrap() {
        return;
    }

    let mut icon_guard = state.icon.lock().unwrap();
    if icon_guard.is_some() {
        return; // already showing — nothing to do
    }

    let Ok(icon) = Image::from_bytes(TRAY_ICON_BYTES) else {
        eprintln!("[claude-mini] failed to decode bundled tray icon");
        return;
    };

    let quit_item = MenuItem::with_id(app, QUIT_ITEM_ID, "Quit Mini Chat for Claude", true, None::<&str>);
    let menu = quit_item.ok().and_then(|q| Menu::with_items(app, &[&q]).ok());

    let app_for_click = app.clone();
    let mut builder = TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Mini Chat for Claude")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window(&app_for_click);
            }
        });

    if let Some(m) = &menu {
        builder = builder.menu(m).on_menu_event(|app, event| {
            if event.id().as_ref() == QUIT_ITEM_ID {
                app.exit(0);
            }
        });
    }

    match builder.build(app) {
        Ok(tray) => *icon_guard = Some(tray),
        Err(e) => eprintln!("[claude-mini] tray icon creation failed: {e}"),
    }
}

/// Show + focus the window and drop the tray icon if present. The only
/// place the icon is removed — pairs with hide_window() above to keep
/// "window visible" and "tray icon present" mutually exclusive.
pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let state = app.state::<TrayState>();
    *state.icon.lock().unwrap() = None; // dropping TrayIcon removes it from the OS tray
}
