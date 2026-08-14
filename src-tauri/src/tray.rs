// System tray icon — shown only while the window is hidden AND the user
// has enabled "Close to tray" in Settings.
//
// By design the two states are mutually exclusive: window visible XOR
// tray icon present, never both, never neither once the app has started.
// That invariant is enforced structurally, not by convention — every
// hide/show in the app (the titlebar close button, the global summon
// shortcut) routes through hide_window()/show_window() below rather than
// calling window.hide()/show() directly, so there's exactly one place
// that reveals the icon and exactly one place that hides it.

use std::sync::Mutex;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{image::Image, AppHandle, Manager};

// Purpose-drawn for 16-32px (bolder strokes, less internal padding, flat
// fill — no gradient) rather than reusing the app icon shrunk down, which
// went mushy in the tray. Same window+pills concept as icons/icon.png.
const TRAY_ICON_BYTES: &[u8] = include_bytes!("../icons/tray.png");
const QUIT_ITEM_ID: &str = "tray-quit";
const TRAY_ID: &str = "claude-mini-tray";

// The icon is built at most once per process and then shown/hidden with
// set_visible(), never rebuilt. Rebuilding per hide looks equivalent but
// isn't: TrayIconBuilder::build() files the icon in the app's resource
// table, so the app itself holds a strong reference for its whole life.
// Dropping our own handle therefore removes nothing — the OS icon stays,
// and the next hide adds another one beside it. (Same reason
// `enabled`-off can't just drop the handle either.)
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
        // while one happens to be showing. Kept (hidden) rather than
        // destroyed, so re-enabling doesn't have to rebuild it.
        let icon_guard = state.icon.lock().unwrap();
        if let Some(icon) = icon_guard.as_ref() {
            let _ = icon.set_visible(false);
        }
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

/// Build the one tray icon this process will ever own. Called at most
/// once — see the note on TrayState for why rebuilding isn't an option.
fn build_icon(app: &AppHandle) -> Option<TrayIcon> {
    let icon = Image::from_bytes(TRAY_ICON_BYTES)
        .inspect_err(|_| eprintln!("[claude-mini] failed to decode bundled tray icon"))
        .ok()?;

    let quit_item = MenuItem::with_id(app, QUIT_ITEM_ID, "Quit Mini Chat for Claude", true, None::<&str>);
    let menu = quit_item.ok().and_then(|q| Menu::with_items(app, &[&q]).ok());

    let app_for_click = app.clone();
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
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

    builder
        .build(app)
        .inspect_err(|e| eprintln!("[claude-mini] tray icon creation failed: {e}"))
        .ok()
}

/// Hide the window. If "close to tray" is on, also raise the tray icon
/// (built on first use, shown again on subsequent hides) that restores
/// the window.
pub fn hide_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }

    let state = app.state::<TrayState>();
    if !*state.enabled.lock().unwrap() {
        return;
    }

    let mut icon_guard = state.icon.lock().unwrap();
    if icon_guard.is_none() {
        *icon_guard = build_icon(app);
    }
    if let Some(icon) = icon_guard.as_ref() {
        let _ = icon.set_visible(true);
    }
}

/// Show + focus the window and hide the tray icon. The only place the
/// icon is hidden on the show path — pairs with hide_window() above to
/// keep "window visible" and "tray icon present" mutually exclusive.
pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let state = app.state::<TrayState>();
    let icon_guard = state.icon.lock().unwrap();
    if let Some(icon) = icon_guard.as_ref() {
        let _ = icon.set_visible(false);
    }
}
