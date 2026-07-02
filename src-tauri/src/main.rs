// Prevents an extra console window from spawning alongside the GUI on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    claude_mini_lib::run()
}
