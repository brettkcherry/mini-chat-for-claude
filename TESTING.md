# Testing

The test bench covers the **Rust/Tauri backend only** (`src-tauri/`). The
frontend (`src/main.js`) is vanilla-JS DOM glue with no bench yet — see
"Future test surface" below.

## Running

From the repo root:

```powershell
.\run-tests.ps1
```

or directly:

```powershell
cd src-tauri
cargo test
```

Both run `cargo test --manifest-path src-tauri/Cargo.toml`; `run-tests.ps1`
just forwards the exit code so it's CI/hook-friendly from the repo root.

## What's covered

### `src-tauri/src/anthropic.rs` — SSE streaming parser
The Anthropic API streams responses as Server-Sent Events: `data: {...}\n\n`
blocks over a raw byte stream that can split anywhere, including mid-event.
The parsing logic (accumulate into a buffer, split on `\n\n`, parse `data: `
lines as JSON, dispatch on `type`) was extracted out of `stream_chat`'s
inline `while let Some(chunk) = stream.next().await` loop into a standalone
`SseParser` struct (`buffer: String`, `push(&mut self, text: &str) ->
Vec<ChunkEvent>`) so it can be driven with plain strings instead of a live
HTTP connection. `stream_chat` now just owns an `SseParser` and forwards
network chunks into `push`, then calls `on_chunk` for each event returned —
same buffer/boundary/dispatch behavior as before, just decoupled from the
network so it's unit-testable.

Covered:
- a single `content_block_delta` → one `ChunkEvent` with the right text
- `message_stop` → one `ChunkEvent { delta: "", stop: true }`
- two events delivered in one `push` → two events, in order
- **an event split across two `push` calls** (the key regression case for
  the buffer-accumulation logic — first half yields nothing, the rest of
  the event arrives in a later chunk and completes it)
- malformed JSON in a `data:` line → ignored, no panic
- a non-`data:` line (e.g. `event: ping`) → ignored
- an event type we don't act on (`content_block_start`) → no event emitted
- a realistic multi-event stream (start → deltas → stop) → deltas
  concatenate to the expected string, final stop event present

Also covered: `RequestBody` serialization shape — `output_config.effort` is
present when `effort` is `Some`, and the `output_config` key is *absent
entirely* when `effort` is `None` (this is load-bearing: a bare top-level
`effort` field gets rejected by the API with a 400).

### `src-tauri/src/sessions.rs` — session ID validation, filename sanitization, serde shape
- `validate_session_id` (extracted from `session_path`'s inline guard) is a
  path-traversal guard: session IDs become filenames, so this is tested
  thoroughly — valid alphanumeric IDs pass; empty IDs, and IDs containing
  `.`, `/`, `\`, spaces, `-`, or non-ASCII characters, are all rejected
  (covers `"../x"`-style traversal attempts).
- `sanitize_title_stem` (extracted from `export_markdown`'s inline filename
  logic) turns an arbitrary session title into a safe filename stem:
  special characters become `_`, length is capped at 40 chars, whitespace
  is trimmed and internal spaces become dashes, and an empty/whitespace-only
  title falls back to `"chat"`.
- `Session` / `SessionMeta` serde: round-tripping a `Session` through
  `serde_json` preserves every field, and the JSON keys are camelCase
  (`createdMs`, `updatedMs`, `messageCount`) as the frontend expects.

## What's deliberately NOT covered, and why

- **The actual network call in `stream_chat`** (the `reqwest` POST + status
  handling). This would require mocking HTTP or hitting the real API in
  tests — out of scope for a fast, deterministic unit bench. The part that
  matters (SSE parsing) is fully covered via `SseParser` instead.
- **`src-tauri/src/secrets.rs`** — thin wrapper around the Windows
  Credential Manager (native OS API). Not meaningfully unit-testable
  without a real credential store; would need an integration test running
  on an actual Windows session.
- **All Tauri command/window wiring** (`commands.rs`, `window.rs`, `lib.rs`,
  `main.rs`) — these are glue that calls into `anthropic.rs`/`sessions.rs`
  and the Tauri runtime. They'd need a running Tauri app context to test
  meaningfully; the logic worth testing has been extracted into the pure
  functions covered above.
- **`src/main.js`** (frontend) — DOM-coupled vanilla JS, no test harness
  set up. See below.

## Extraction rationale

Both extractions (`SseParser`, `validate_session_id`, `sanitize_title_stem`)
follow the same pattern: pull the *pure* logic (string/byte processing, no
I/O, no `AppHandle`) out of the function that tangles it with a side effect
(network I/O, filesystem/`AppHandle` access), and have the original function
call the extracted one. Behavior is unchanged — same inputs produce the same
outputs and side effects — but the pure core is now driveable with plain
Rust values in `#[cfg(test)]` instead of requiring a live network connection
or a real Tauri `AppHandle`/filesystem.

## Future test surface

- **Frontend (`src/main.js`)**: extract pure helper functions — e.g.
  session title derivation, opacity/animation math — into a separate module
  and add a Vitest + jsdom bench for them. `main.js` today is almost
  entirely DOM event wiring, so this would mean carving out the handful of
  pure calculations first.
- **Integration tests for Tauri commands** (`commands.rs`): would need a
  way to spin up (or mock) an `AppHandle`/`WebviewWindow` — worth revisiting
  if Tauri's test utilities make this cheap, but not attempted here to keep
  the bench dependency-free and fast.
