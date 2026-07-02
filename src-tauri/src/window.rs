// Platform-specific window chrome lives here. Every `#[cfg(target_os = ...)]`
// in this codebase should live in this file. If you find yourself adding one
// elsewhere, refactor it back into here.

use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WebviewWindow};

/// Default window dimensions in logical pixels. Must match tauri.conf.json —
/// these exist because the config values failed to apply reliably (see below).
const DEFAULT_WIDTH: f64 = 380.0;
const DEFAULT_HEIGHT: f64 = 560.0;

/// Apply any post-creation tweaks the window needs.
///
/// - Places the window on the very first launch (no saved state file yet).
///   tauri-plugin-window-state handles subsequent launches automatically.
/// - Both `center: true` in tauri.conf.json AND `window.center()` proved
///   unreliable on a HiDPI 2880x1800 display: the window twice opened in
///   the bottom-right corner at the wrong size (501x320, then 474x355
///   physical px instead of 380x560 logical). So we place it by hand:
///   explicit size first, then a centered position computed from the
///   monitor's own geometry. Deterministic, no trust in config-layer math.
/// - Hook for future Mica/Acrylic blur on Windows 11.
pub fn apply_platform_chrome(app: &AppHandle, window: &WebviewWindow) {
    if !has_saved_window_state(app) {
        place_first_launch(window);
    }

    #[cfg(target_os = "windows")]
    {
        apply_windows_chrome(window);
    }

    #[cfg(target_os = "linux")]
    {
        apply_linux_chrome(window);
    }

    // THE actual fix for "window never appears" on this machine: despite
    // `visible: true` in tauri.conf.json, the window was created with
    // visible=False (confirmed via EnumWindows — the frameless+transparent
    // combo seems to skip the initial show). Show it unconditionally; cheap
    // no-op if it's already visible.
    if let Err(e) = window.show() {
        eprintln!("[claude-mini] window.show() failed: {e}");
    }
    let _ = window.set_focus();
}

/// Explicitly size the window, then center it using monitor geometry we
/// compute ourselves. All position math in physical pixels.
fn place_first_launch(window: &WebviewWindow) {
    // Force the intended size — this is the step the config layer botched.
    if let Err(e) = window.set_size(LogicalSize::new(DEFAULT_WIDTH, DEFAULT_HEIGHT)) {
        eprintln!("[claude-mini] set_size failed: {e}");
    }

    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());

    if let Some(m) = monitor {
        let scale = m.scale_factor();
        let m_size = m.size(); // physical px
        let m_pos = m.position(); // physical px
        let win_w = (DEFAULT_WIDTH * scale) as i32;
        let win_h = (DEFAULT_HEIGHT * scale) as i32;
        let x = m_pos.x + ((m_size.width as i32 - win_w) / 2).max(0);
        let y = m_pos.y + ((m_size.height as i32 - win_h) / 2).max(0);
        eprintln!(
            "[claude-mini] placing: monitor pos=({},{}) size={}x{} scale={} -> window ({x},{y}) {win_w}x{win_h}",
            m_pos.x, m_pos.y, m_size.width, m_size.height, scale
        );
        if let Err(e) = window.set_position(PhysicalPosition::new(x, y)) {
            eprintln!("[claude-mini] set_position failed: {e}");
        }
    } else {
        eprintln!("[claude-mini] no monitor info; falling back to center()");
        let _ = window.center();
    }
}

/// Has tauri-plugin-window-state ever saved a window-state file for us?
/// If yes, the plugin has already restored position/size, and we should NOT
/// override it by centering. If no, this is a first launch — center.
///
/// The plugin saves to `<app_config>/.window-state.json` by default.
fn has_saved_window_state(app: &AppHandle) -> bool {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join(".window-state.json").exists())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
fn apply_windows_chrome(_window: &WebviewWindow) {
    // History, so nobody re-tries the two dead ends already explored here:
    //
    // 1. DWMWA_WINDOW_CORNER_PREFERENCE = DWMWCP_DONOTROUND — squared off
    //    DWM's native shadow instead of rounding it, so a square silhouette
    //    peeked out from behind our round CSS content. Worse, not better.
    // 2. Native shadow off (`"shadow": false`) + a hand-drawn CSS
    //    box-shadow on `.app` — CSS box-shadow does not composite cleanly
    //    through Tauri's transparent window on Windows; it rendered as a
    //    hard-edged color halo (the desktop showing through unblended)
    //    instead of a soft gradual fade.
    //
    // The actual fix needed no Rust code at all: `"shadow": true` (native,
    // always smooth — DWM draws it directly, no webview compositing) plus
    // matching our own `--radius-window` CSS variable to Windows 11's own
    // system corner radius (~8px) instead of fighting DWM over which
    // radius wins. Same radius on both sides means there's nothing left
    // to visibly disagree.

    // TODO(v0.2): wire up window_vibrancy crate for Mica on Windows 11.
    // The CSS backdrop-filter blur is good enough for now.
}

#[cfg(target_os = "linux")]
fn apply_linux_chrome(_window: &WebviewWindow) {
    // No-op for now. Linux compositors handle transparency at the WM level.
}
