# Runs the Rust backend test suite for Mini Chat for Claude.
# Frontend (src/main.js) has no test bench yet — see TESTING.md.

cargo test --manifest-path src-tauri/Cargo.toml
exit $LASTEXITCODE
