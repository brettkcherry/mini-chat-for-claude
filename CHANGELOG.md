# Changelog

Newest first. This project ships one rolling release — only the latest version
gets fixes ([SECURITY.md](./SECURITY.md)).

## [0.4.2] — 2026-08-12

### Security

- **Exporting a transcript no longer lets the frontend pick the path.** The save
  dialog now runs in Rust: the frontend passes a title and the contents, and gets
  back the path that was written. Previously it passed a path to a `fs::write`,
  which made the dialog a convention the JS side observed rather than a boundary
  the Rust side enforced — anything controlling the webview could have skipped it
  and written anywhere the user could, the Startup folder included. The
  precondition was a DOMPurify bypass, which is unlikely and is exactly the kind
  of thing that shouldn't be the only thing standing in the way.

  Three commands collapsed into one as a result, and dropping the JS-side dialog
  also removed the `dialog:allow-save` capability and the
  `@tauri-apps/plugin-dialog` dependency — the permission no longer exists to
  misuse.
- **`npm ci --ignore-scripts` in CI**, including the release job that holds the
  update signing key. A compromised package's payload almost always runs from an
  install hook, before any of our own code does. Verified that no package in this
  tree declares one, so nothing is suppressed.
- **Licence gate** (`cargo deny check licenses`, policy in `src-tauri/deny.toml`).
  `cargo about` generates the notices; this fails the build when a dependency
  arrives under terms an MIT app can't ship. Nobody diffs a 500-entry notices
  file, so the generated report was never going to catch a transitive crate that
  appeared during a routine `cargo update`.
- **Dependabot cooldown.** New versions wait a few days before being offered.
  Compromised packages are usually caught and pulled within a day or two, and the
  people they reach are the ones who upgraded within hours of publication.
- **Release job gated behind required-reviewer approval**, and the update
  signing key moved off repository-level secrets onto that environment's own —
  narrowing which workflow runs can ever read it to ones a human approved.

### Fixed

- The crate declared no `license` field, so its own licence was simply unstated —
  which tooling correctly reads as unlicensed rather than as permissive. Now
  `license = "MIT"`, matching LICENSE, plus `publish = false` since this is an
  application and nothing here is meant for crates.io.

### Changed

- **New app icon.** Two overlapping message bubbles in crimson, replacing the
  burnt-amber window+pills design. The front bubble carries the reply tail and
  stays darkest; the back bubble sits behind it in a lighter tint of the same
  hue, separated by a thin keyline gap so the overlap reads as depth instead
  of a notch cut out of one shape. Chosen after testing shape, value
  structure, and hue independently — this combination was the one that stayed
  legible all the way down to a 16px taskbar icon.

## [0.4.1] — 2026-08-04

Five fixes, all found by using the app rather than by any test: in most cases
it was behaving correctly and telling you nothing.

### Fixed

- **Claude now says when it's thinking.** On the newer models, thinking happens
  before any visible text and produces none of its own — so a reply could sit
  for 30 seconds or more showing nothing but a blinking cursor, which is
  indistinguishable from the app having hung. The reply bubble now says
  "Thinking…" until the first word arrives.
- **An empty reply says so.** If a turn produced no text at all, the bubble was
  silently removed — leaving your own message with nothing after it and no way
  to tell whether the app had failed or the model had simply said nothing.
- **Long replies no longer freeze the window.** A reply was re-parsed in full on
  every streamed token — parser, then sanitizer, then a complete DOM
  replacement, hundreds of times over. The cost grew with the square of the
  reply, so a long answer could pin the interface long enough that the titlebar
  buttons stopped responding. Measured on a 9,700-character reply: 812ms of
  parsing where 0.7ms was needed. Now painted on a throttle — visually
  identical, and the cost no longer scales with length.
- **Settings and Sessions could open invisibly.** Both cards were inserted at
  the top of the scrollable transcript with no scroll reset, so opening either
  from a chat long enough to be scrolled to the bottom landed the card off-screen
  above the fold. The button looked like it did nothing; it had actually worked.

### Added

- **Export now opens a native Save dialog** instead of always writing to
  `Documents\Mini Chat for Claude\` with no way to redirect it. The dialog
  still defaults to that folder and filename — you can just hit save — but any
  folder and name are now yours to pick.

### Changed

- The sessions card now says which chat the export buttons act on. They export
  the chat you have open, but sat directly above a list of every saved chat,
  so the layout implied you were exporting a row from the list.
- **New app and tray icon.** The old one was tuned to look good large and
  went muddy in the taskbar and tray, where you actually see it most. New
  design: a floating window with two message pills, in a warm burnt-amber
  gradient with real contrast, plus a separate simplified version just for
  the tray so it stays sharp at that size.

## [0.4.0] — 2026-08-04

### Fixed

- **Replies no longer lost when a multi-byte character lands on a network chunk
  boundary.** An em dash, curly quote, emoji or accented name could straddle two
  chunks; decoding each one separately then failed, killing the request and
  replacing the partial reply with an error.
- **A reply interrupted mid-stream is no longer stranded.** An error after
  content started arriving used to end the stream with no terminal event: the
  bubble streamed forever and the text already received never entered the
  conversation history. Every turn now ends exactly once, however it ends.
- **Long replies are no longer silently truncated.** The output cap was
  hardcoded at 4096 tokens for every model; it now comes from each model's own
  reported ceiling, and a reply that hits the cap says so.
- **API errors are readable.** Out of credit, bad key, and rate limits are
  explained in words instead of raw JSON.
- **A declined request explains itself** rather than leaving an empty bubble.

### Added

- **Stop button.** Halts a reply mid-stream (or `Esc`). Closes the connection,
  so generation stops upstream and billing with it; the partial reply is kept.
- **Prompt caching** — repeated conversation context now costs roughly a tenth
  of what it did on every turn after the first.
- **New icon** — a teal chat window echoing the app's own composer, replacing a
  lettermark that sat too close to Anthropic's colour identity.
- Screenshots in the README.
- **Screen-reader support** — replies are announced when they complete, and
  every control is labelled. The transcript deliberately isn't a live region:
  it re-renders on every token, so marking it live would read a growing partial
  sentence hundreds of times per reply.
- The "unofficial, not affiliated with Anthropic" notice now appears in the app
  itself, not only in the README.
- Something small and undocumented, for the curious. 🌊

### Changed

- All dependencies current, including Vite 6→8, `tauri-action` 0→1, and
  `actions/checkout` and `actions/setup-node` 4→7.
- `cargo audit` runs with no ignore list. The `quinn-proto` advisory was
  previously excepted because fixing it forced a `rand` major bump across the
  tree; that bump has been taken.
- Branch protection on `main`, issue templates, contributing guide.

## [0.3.0] — 2026-08-02

### Added

- Dark / light / system theme.
- Close-to-tray, with a setting for whether ✕ hides or quits.
- Single-instance enforcement — launching again restores the running window
  instead of starting a second copy.
- App version shown in Settings.
- "Delete all chat history", behind a confirm step.

### Security

- Every GitHub Action in the release workflow pinned to a full commit SHA. That
  job holds the key authorizing auto-updates to every installed copy, so a
  retagged action would be a direct path to shipping a malicious update. One of
  the five had been pinned to a branch.
- CI gate on every push and PR (`cargo test`, `npm audit`, `cargo-audit`), which
  immediately surfaced two real high-severity advisories — `quick-xml` via
  `plist`, and `serde_with` — both fixed.
- DOMPurify to 3.4.12, closing two CVEs in the sanitizer model output passes
  through.
- **The uninstaller now removes your API key.** "Delete application data" only
  cleared directories; the key lives in Windows Credential Manager and survived
  it. Skipped during auto-updates, which re-run the same uninstaller.
- The `ANTHROPIC_API_KEY` environment fallback is compiled out of release
  builds — in a shipped app it's an injection point, not a convenience.
- CSP tightened with `form-action`, `base-uri` and `object-src`, none of which
  inherit from `default-src`.
- Session IDs re-validated on read rather than trusted from disk.

### Legal

- `NOTICE.md`, per-crate licenses for all 547 Rust dependencies, `SECURITY.md`,
  and publisher/copyright metadata in the installer.

## [0.2.1] — 2026-07-30

### Added

- Model picker populated live from `/v1/models` — IDs *and* per-model effort
  capabilities, so the list can't go stale and retired models drop off on their
  own.
- Model picker is a scrollable dropdown rather than click-to-cycle.
- Model choice persists, keyed by ID rather than list position.

### Fixed

- Effort level stored canonically lowercase end to end, so a display-only
  capitalisation can't silently break the value sent to the API.

## [0.2.0] — 2026-07-30

First public release, as "Mini Chat for Claude" (renamed from "Claude Mini";
internal identifiers deliberately unchanged so existing chats and saved keys
survived).

### Added

- Auto-updater with Ed25519-signed artifacts — the app refuses any update it
  can't verify.
- Sessions: every chat autosaves locally; browse, reload, delete.
- Export a chat as Markdown, to clipboard or file.
- Effort level control, per-model aware.
- Settings card: always-on-top, raw-text mode, opacity.
- MIT license, public README, CI that builds and signs on every version tag.

### Security

- CSP tightened from `null` to an explicit policy.

## [0.1.0] — 2026-06-12

Initial build. Frameless always-on-top window, streaming replies, API key in
Windows Credential Manager, markdown rendering, global summon shortcut,
container-query responsive layout.
