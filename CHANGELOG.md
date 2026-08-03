# Changelog

All notable changes to Mini Chat for Claude. Dates are release dates; the
newest version is at the top.

This project ships one rolling release — only the latest version receives
fixes. See [SECURITY.md](./SECURITY.md).

## [Unreleased]

### Fixed

- **A reply is no longer lost when a multi-byte character lands on a network
  chunk boundary.** An em dash, a curly quote, an emoji, or an accented name
  could straddle the split between two network chunks; decoding each chunk on
  its own then failed, aborting the request and replacing the partial reply on
  screen with a decode error. Bytes are now buffered across chunks.
- **A reply interrupted mid-stream is no longer stranded.** An error raised
  after content started arriving (an overloaded API, a dropped connection) used
  to end the stream with no terminal event: the bubble kept its blinking caret
  forever, and the text already received never made it into the conversation
  history. Every turn now ends with exactly one stop event, whatever happened.
- **Long replies are no longer silently truncated.** The output cap was
  hardcoded at 4096 tokens for every model. It now comes from each model's own
  reported ceiling, and a reply that does hit the cap says so instead of just
  stopping mid-sentence.
- **API errors are readable.** "Credit balance too low", a rejected key, and
  rate limiting are explained in plain words instead of raw JSON.
- **A declined request explains itself** rather than leaving an empty bubble.

### Added

- **Stop button.** The send button becomes a stop control while a reply
  streams; `Esc` does the same. Stopping closes the connection — generation
  halts upstream and billing stops with it — and keeps the partial reply.
- **Prompt caching.** Every turn re-sends the whole conversation, so the same
  prefix was being paid for at full price on every message. It is now cached
  and read back at roughly a tenth of the cost on subsequent turns.

### Changed

- All dependencies current, including four major bumps: Vite 6→8 (which
  required swapping the minifier setting, since Vite 8 replaced esbuild with
  oxc), `tauri-action` 0→1, and `actions/checkout` and `actions/setup-node`
  4→7.
- `cargo audit` now runs with no ignore list. The `quinn-proto` advisory was
  excepted in CI because fixing it forced a `rand` major bump across the tree;
  that bump has been taken and everything still passes.
- Branch protection on `main`, issue templates, and a contributing guide.

## [0.3.0] — 2026-08-02

### Added

- Dark/light/system theme toggle, with a full light palette.
- Close-to-tray, with a Settings toggle for whether ✕ hides or quits.
- Single-instance enforcement — launching the app again restores the running
  window instead of starting a second copy.
- App version shown in Settings.
- "Delete all chat history" control, behind an explicit confirm step.

### Security

- Every GitHub Action in the release workflow pinned to a full commit SHA. The
  release job holds the update-signing key, so a retagged action would be a
  direct path to shipping a malicious update.
- CI gate on every push and PR: `cargo test`, `npm audit`, and `cargo-audit`.
- Fixed two high-severity advisories found by that gate: `quick-xml` (via
  `plist`) and `serde_with`, both transitive through `tauri-utils`.
- Bumped DOMPurify to 3.4.12 (two live CVEs).
- Tightened CSP with `form-action`, `base-uri`, and `object-src` — none of
  which inherit from `default-src`.
- The `ANTHROPIC_API_KEY` environment-variable fallback is now compiled out of
  release builds. In a shipped app it is an injection point, not a
  convenience.
- The uninstaller now removes the stored API key from Windows Credential
  Manager when "delete application data" is ticked — the built-in cleanup only
  clears directories, so the key survived it. Deliberately skipped during
  auto-updates, which re-run the same uninstaller.
- Session IDs are re-validated on the way out of storage, not just trusted
  from disk.
- Added `NOTICE.md`, per-crate license texts for all 547 Rust dependencies, and
  `SECURITY.md`.

## [0.2.1] — 2026-07-30

### Added

- Model picker is populated live from `/v1/models` — model IDs and per-model
  effort capabilities are read from the API rather than hardcoded, so the list
  can't go stale and retired models drop off on their own.
- Model picker is now a scrollable dropdown rather than click-to-cycle.
- Model choice persists across restarts.

### Fixed

- Effort level is stored canonically lowercase end to end, so a display-only
  capitalization change can no longer break the value sent to the API.

## [0.2.0] — 2026-07-30

### Added

- First public release. Renamed from "Claude Mini" to "Mini Chat for Claude".
- Auto-updater, with Ed25519-signed release artifacts.
- Redesigned titlebar plus a Settings card: always-on-top, raw-text mode,
  opacity slider.
- Sessions: every chat autosaves locally; browse, reload, delete.
- Export a chat as Markdown, to the clipboard or a file.
- Effort level control.
- MIT license, public README, and CI that builds and signs on every tag.

### Security

- CSP tightened from `null` to an explicit policy.

## [0.1.0] — 2026-06-12

Initial build. Frameless always-on-top window, streaming replies from the
Anthropic API, API key in Windows Credential Manager, markdown rendering,
global summon shortcut, container-query responsive layout.
