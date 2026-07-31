// Prevents an extra console window from spawning alongside the GUI on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The NSIS uninstaller calls us with this flag before it removes any
    // files (see installer-hooks.nsh). Handle it here, ahead of `run()`, so it
    // never reaches the Tauri builder — the single-instance plugin would
    // otherwise hand the arguments off to an already-running GUI instance and
    // return immediately, deleting nothing.
    if std::env::args().skip(1).any(|arg| arg == "--uninstall-cleanup") {
        claude_mini_lib::uninstall_cleanup();
        return;
    }

    claude_mini_lib::run()
}
