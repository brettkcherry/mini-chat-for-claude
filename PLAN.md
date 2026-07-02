# Claude Mini Player — PLAN.md

> A floating desktop widget for chatting with Claude. Frameless, always-on-top, resizable, responsive. Inspired by Spotify's mini player.

---

## The vision

A tiny, beautiful window that lives on the desktop. Just a chat bar at the bottom and a scrolling message window above it. That's it. No tabs, no sidebar, no settings drawer cluttering the view. You hit a keyboard shortcut, it pops up, you type, Claude replies, you keep going.

It should feel like a *peripheral* — something that sits next to your actual work, not something you switch to.

### The Spotify reference

The Spotify mini player nails three things we want to copy:

1. **Frameless / chromeless** — no OS title bar, custom drag region, the app *is* the content
2. **Resizable with smart constraints** — drag any edge, min/max bounds, the layout reflows beautifully
3. **Design responsiveness** — at a large size it's a full player; at a small size it collapses to album art + controls; at the smallest, it's basically a single row. The transitions are tasteful, not abrupt.

That last point is the bar we're aiming for. The widget should look *intentional* at every size between ~280px wide and ~600px wide.

---

## Stack — DECIDED

**Tauri.** Rust backend + system webview (WebView2 on Windows, WKWebView on macOS), ~5MB final binary, frameless / always-on-top / transparency all built-in. Frontend is plain HTML/CSS/JS (framework TBD).

## Platform targets — DECIDED

**Windows v0.1 → Linux v0.2 → macOS *if/when a Mac is available*.**

Brett doesn't have a Mac, so Mac is on indefinite hold. Linux replaces it as v0.2 — and it's actually easier than Mac would have been.

### Why Linux is the easy second target

- Uses `webkit2gtk` (system webview) — same Tauri APIs as Windows
- **No code signing required** — no Apple-equivalent gatekeeper, no $99/year fee
- Build on any Linux box or GitHub Actions `ubuntu-latest` (free on public repos)
- Ships as `.deb`, `.rpm`, or `.AppImage` (AppImage = portable, no install)
- Quirks are minor: webkit2gtk renders some CSS slightly older than Edge WebView2; window transparency requires the user to have a compositor running (true on essentially all modern GNOME/KDE)

### Cross-platform cost breakdown (Win → Linux)

**Free (code is identical on both):**
- Window APIs — frameless, always-on-top, resize, transparency, min/max bounds
- Frontend HTML/CSS/JS
- Anthropic API streaming calls
- Keychain via `keyring` crate (uses libsecret / GNOME Keyring / KWallet on Linux, Credential Manager on Windows)
- Global keyboard shortcut plugin
- File persistence

**Real costs for Linux support:**
1. **CSS quirks.** webkit2gtk is generally a few WebKit versions behind. Container queries are fine, but exotic CSS may render differently. Test, don't assume.
2. **Tray icon / global shortcut conventions** differ between desktop environments (GNOME, KDE, Cinnamon, tiling WMs). Tauri smooths most of this but not all of it.
3. **Dependency story.** `.deb` and `.rpm` packages need their libwebkit2gtk deps declared; AppImage bundles them. Tauri's bundler handles this.

**Engineering discipline to keep this cheap:** isolate every platform-specific decision into a single Rust module that branches on `cfg!(target_os = ...)`. Window setup, blur effects, tray behavior — all in one place. Everything else stays portable.

### Rough effort estimate

| Phase                                | Effort                |
|--------------------------------------|-----------------------|
| Windows MVP (v0.1)                   | ~1–2 weekends         |
| Linux port (v0.2) — code only        | a few hours           |
| Linux packaging (.deb / AppImage)    | ~half a day           |
| CI pipeline for both                 | ~half a day           |

Linux is genuinely the easiest "next platform" Tauri offers. No paid certs, no notarization, no platform-specific account to register for.

---

## Architecture sketch

```
┌─────────────────────────────────────────┐
│  Frameless window (drag region = top)   │
│  ┌───────────────────────────────────┐  │
│  │                                   │  │
│  │   Message history (scrollable)    │  │
│  │   - user / assistant bubbles      │  │
│  │   - streaming token rendering     │  │
│  │   - markdown + code blocks        │  │
│  │                                   │  │
│  ├───────────────────────────────────┤  │
│  │  [ chat input ]              [↵]  │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
       ↑ resize handle on every edge
```

### Talking to Claude

Two routes:
- **Anthropic API direct** (recommended) — use the user's API key, call `claude-opus-4-7` / `claude-sonnet-4-6` / `claude-haiku-4-5` with streaming. Store the key in the OS keychain (Tauri has `tauri-plugin-stronghold`, Electron has `keytar`). This is the cleanest path and gives us full control.
- **Wrap claude.ai in a webview** — quick win, but fragile (DOM changes break it), and we lose control over the chrome we're trying to strip.

Go with the API.

### Responsive layout strategy

The Spotify magic is **container queries**, not media queries — the layout responds to the window's own width, not the viewport. Modern CSS supports this natively (`@container`). Plan on three breakpoints:

| Size      | Width       | Layout                                                |
|-----------|-------------|-------------------------------------------------------|
| Compact   | 280–360 px  | Single-line input, hide model selector, smaller font  |
| Standard  | 360–480 px  | Full input row with model badge, normal message density |
| Expanded  | 480 px+     | Roomier bubbles, optional metadata (timestamps, tokens) |

Height-wise: just let the message area flex. The input bar is always pinned to the bottom.

---

## MVP scope

What ships in v0.1:

- [ ] Frameless, always-on-top window (toggleable)
- [ ] Resizable with min/max bounds (e.g., 280×320 min, 800×900 max)
- [ ] Custom drag region (top strip, ~24px)
- [ ] Single message thread, scrollable
- [ ] Streaming responses from Anthropic API
- [x] API key stored securely (OS keychain, not plaintext)
- [ ] Model picker (Opus 4.7 / Sonnet 4.6 / Haiku 4.5)
- [x] Global keyboard shortcut to show/hide (`Ctrl+Shift+Space`)
- [ ] Conversation persists across show/hide within a session
- [ ] Container-query-based responsive layout (3 breakpoints)
- [x] Markdown + code block rendering in messages (marked + DOMPurify, live re-render while streaming)

## Stretch goals (post-MVP)

- Snap to screen corners
- Acrylic / Mica blur background on Windows 11
- Conversation history (persist across app restarts, browseable)
- "New chat" button
- Image attachments (paste/drag-drop)
- Tool use / MCP integration
- Multi-conversation tabs (probably *don't* do this — kills the "tiny widget" vibe)
- Optional "compact-only" mode that snaps the window down to a single chat-bar row when not in use
- Light/dark theme following OS

---

## Open questions

- **Which API key flow?** Settings panel? First-run wizard? Just a small icon that opens a key field?
- **Persistence model?** SQLite (Tauri has a plugin), flat JSON file, or in-memory only for v0.1?
- **Branding?** Just "Claude" mark, or a fresh name for the mini app?
- **Update channel?** Tauri has an auto-updater; do we want that on day one?
- ~~**Cross-platform target?**~~ **DECIDED:** Windows v0.1, Linux v0.2. Mac on hold (no Mac available).

---

## Conversation notes — 2026-05-21

**Brett's pitch:** A mini app / widget for Claude chat that sits on the desktop. Just a chat bar and a text window. Inspired by Spotify's mini player — resizable, beautifully design-responsive.

**Claude's take:** Totally doable. The shell (frameless, always-on-top, resizable, drag-to-move) is a solved problem in both Tauri and Electron. The *interesting* engineering is the responsive layout — making it look great at every width. Container queries are the modern tool for that.

**Recommended stack:** Tauri. ~5MB binary vs Electron's ~100MB matters when you're pitching this as a "tiny widget." Backup: Electron, if Rust is a hurdle.

**Talking to Claude:** Anthropic API direct (streaming), with the API key in OS keychain. Don't wrap claude.ai in a webview — fragile and undoes the whole point of stripping the chrome.

**Next step:** ~~Decide stack~~ ✅ Tauri. ~~Decide platform target~~ ✅ Windows first, Mac later. Now: scaffold the window + first streaming message. Everything after that is iteration on the layout.

### Session 2026-06-12 (part 3) — first real API contact + keyring lands

**The $5 gauntlet (worth recording for the comedy):**
1. First 401: key was pasted as `sk-ant-sk-ant-...` — Brett swapped only the
   `YOUR-REAL-KEY-HERE` part of the placeholder, keeping its `sk-ant-` prefix.
   Diagnosed via `.Substring(0,12)` → `sk-ant-sk-an`. Fixed with `.Substring(7)`.
2. Then 400: "credit balance too low" — valid key, but API credits are a
   separate wallet from Claude Pro. `/v1/models` worked free; messages don't.
   Brett bought $5 of credits.
3. Bonus: the live `/v1/models` response confirmed every model ID in our
   picker exactly: `claude-fable-5`, `claude-opus-4-8`, `claude-sonnet-4-6`,
   `claude-haiku-4-5-20251001`. ✓

**Keyring milestone (kills the env-var dance):**
- `secrets.rs`: save/load/delete via `keyring` crate → Windows Credential
  Manager (service `claude-mini`, account `anthropic-api-key`). Save
  validates: empty, doubled `sk-ant-sk-ant-` prefix (ask us how we know),
  missing `sk-ant-` prefix, suspiciously short.
- Commands: `save_api_key`, `has_api_key`, `delete_api_key`. `send_chat`
  resolves key as: credential store → `ANTHROPIC_API_KEY` env var → friendly
  error pointing at the key button.
- UI: key button in titlebar opens a setup card (password input, validation
  errors inline). First run with no key: composer disabled + card auto-shown.
- Cargo note: Linux port must use the `sync-secret-service` keyring feature,
  NOT `linux-native` (kernel keyutils loses keys on reboot).

**FIRST REAL CONVERSATION — 2026-06-12 🚀**
Brett's $5 landed and the widget streamed its first real Claude replies
("Hello, this is a test!" → live streamed response). Core loop verified:
keyboard → Tauri IPC → Rust SSE client → Anthropic → chat-chunk events →
streaming bubble. The reply arrived full of raw markdown syntax, which
immediately motivated the next milestone:

**Markdown rendering (same session):**
- `marked` (breaks+gfm) + `DOMPurify` — model output is ALWAYS sanitized
  before hitting innerHTML.
- Re-renders the accumulated markdown on every streaming chunk, so
  formatting appears live mid-stream.
- History commits the raw markdown (not HTML) so API context stays clean.
- Link clicks are blocked from navigating the webview (would replace the
  app UI); TODO: tauri-plugin-opener to open in system browser.
- Compact markdown CSS: headings scaled for widget size, code blocks with
  horizontal scroll, tables, blockquotes with accent border.

**Polish round (same session, Brett's requests):**
- **Link clicks copy the URL** (clipboard) with a fading "Copied" tag —
  cursor shows the copy icon over links. Replaces the old "block and do
  nothing" behavior. System-browser opening still a future option via
  tauri-plugin-opener.
- **Raw mode toggle** (`</>` button in titlebar): switches every Claude
  bubble between rendered markdown and bare-bones raw text (mono font,
  pre-wrap). Implementation keeps the raw markdown in `dataset.raw` on
  each bubble, so toggling is lossless in both directions and applies to
  past + future messages. Preference persists via localStorage. Zero
  changes to the data flow — history always stores raw markdown
  regardless of display mode.
- Brett's design verdict: "I really love the design… it's really cute 😄"

**Key-save confirmation UX (Brett: "it didn't give any kind of confirmation"):**
- Old behavior: "✓ Saved" flashed for 700ms then the card closed — too easy
  to miss. (His save HAD worked — verified via `cmdkey /list` showing the
  `anthropic-api-key.claude-mini` credential.)
- New: `api_key_status` command returns { stored, suffix (last 4 chars),
  env_fallback } — never the full key. The 🔑 card now opens showing
  current state ("✓ A key is saved (ends in …abcd)"), save confirmation
  lingers 2.2s with the suffix, and a "Remove saved key" button appears
  when one exists. Clicking 🔑 anytime = instant verification.
- Verified working by Brett: pin/always-on-top "flawless", raw toggle a hit.

**Click-outside dismisses the 🔑 card** — except in first-run/no-key state,
where the card stays pinned (dismissing it there would strand the user
with a disabled composer).

**RELEASE BUILD — v0.1 SHIPPED 2026-06-12:** `npm run tauri build` →
`Claude Mini_0.1.0_x64-setup.exe` (1.8 MB installer, 4.9 MB exe) at
`src-tauri/target/release/bundle/nsis/`. One build break on the way: my
top-level `await listen(...)` passed dev but broke the es2021 production
target — fixed by dropping the needless await + bumping target to es2022
(WebView2 is evergreen). Unsigned (personal use; SmartScreen may warn
once). Every MVP checkbox ticked. CSP tightening deferred until/unless
this distributes beyond Brett's machine.

**Custom icon — DONE 2026-06-12:** Brett's design direction: pale Claude
orange, bigger C. Generated 4 exact-hex variants (e7b19b/f3c9b7 ×
white/rust C), compared at taskbar size on dark+light strips. **Winner:
variant 3 — #e7b19b plate, #97512f rust C** (white C ghosted at 32px on
light backgrounds; Brett's "f3c9b7 too light?" instinct was right for
white, the rust C rescued both). Two rounds of optical centering: text
line-box centering sits cap-height glyphs ~4-5% low; final offsets
x −0.6%, y −2.8% of canvas. Source: `icon-drafts/final_v3.png` →
`npm run tauri icon` → all formats. Font: Segoe UI Bold (system font —
NOT Claude's brand font; Anthropic uses licensed Styrene which we can't
bundle. Revisit if a closer free face matters later).

### Session 2026-07-02 — v0.2 features: effort, sessions, export

Brett's asks: effort adjustability, save/send/export a chat, new-session
button — and the answer to "do we lose previous sessions?" was YES (memory
only), which this session fixes.

**Effort levels:**
- New badge under the model badge in the composer. Cycles auto → low →
  medium → high → (xhigh) → max; "auto" sends nothing (API default).
- Per-model support enforced from the live /v1/models data we captured
  2026-06-12: Fable 5 + Opus 4.8 = all levels; Sonnet 4.6 = no xhigh;
  Haiku 4.5 = no effort at all (badge hides entirely).
- Rust: `effort: Option<&str>` on the request body,
  `skip_serializing_if = None` so unsupported models never see the field.
- ~~⚠️ UNVERIFIED: assumed top-level `effort`~~ **RESOLVED 2026-07-02:**
  Brett hit `400: "effort: Extra inputs are not permitted"`. Looked up the
  live API reference (platform.claude.com/docs/en/api/messages): the real
  shape is `output_config: { effort: "<level>" }`. Fixed in anthropic.rs
  with an `OutputConfig` wrapper struct, still `skip_serializing_if None`.
- Same session: effort pill restyled per Brett — moved to the right of the
  model pill (horizontal row, composer back to single-row height), plain
  text, no ⚡, no accent color; visually identical to the model pill.
- Choice persists in localStorage; resets to auto if the new model
  doesn't support the current pick.

**Sessions (nothing is lost anymore):**
- Every chat autosaves to `<app_data>/sessions/<id>.json` after each user
  message and each completed reply. File-per-session, no DB — the user
  can inspect/back up/delete files by hand.
- ➕ titlebar button = new chat (old one is already saved; just resets).
- 🕘 titlebar button = sessions card: lists saved chats (title from first
  user message, date, message count), click to reload (restores model
  too), ✕ per row to delete. Current session marked "· current".
- Session ids sanitized in Rust (alphanumeric only — ids are filenames).

**Export:**
- In the sessions card: "Copy chat as Markdown" (clipboard) and "Save
  chat as .md file" → writes to `Documents/Claude Mini/<title>-<ts>.md`
  via `export_chat` command; full path shown on success.

**Quit affordance (discovered via build failure):**
- The v0.2 rebuild failed: `Access is denied` replacing the release exe —
  Brett's running widget held the lock. Investigating exposed a real gap:
  ✕ only hides the window, so a release build was unkillable outside Task
  Manager.
- Fix: `quit_app` command (`app.exit(0)`). Shift+click ✕ quits (plain
  click still hides; tooltip documents both), plus a "Quit Claude Mini"
  button at the bottom of the sessions card.
- Brett approved killing the running old-build instance (its chat was
  pre-autosave and unrecoverable — the last conversation this app will
  ever lose).
- v0.2 installer built clean 2026-07-02 00:25: 1.78 MB setup, 4.6 MB exe.

### Session 2026-07-02 (part 2) — SHIPPING PREP: "Mini Chat for Claude" v0.2.0

Brett: "make it shippable, able to be seen and had by the whole world."

**Decisions (Brett's, via Q&A):**
- **Name: "Mini Chat for Claude"** (his pick; started as "Mini for Claude",
  upgraded mid-flight for discoverability — self-describing in search/
  Start menu). Trademark-safe "X for Claude" pattern instead of "Claude
  Mini" which risked confusion/takedown as Claude is Anthropic's mark.
- **License: MIT** © 2026 Brett Cherry.
- **Signing: unsigned first** ($0; SmartScreen "More info → Run anyway";
  reputation builds with downloads). Microsoft Store dev account ($19)
  later — Brett will sign up when ready; structure kept Store-compatible.
- **Channels: GitHub Releases + winget + in-app auto-updater.**

**Compat-critical constants that did NOT change in the rename** (changing
them would orphan user data): identifier `com.brettcherry.claudemini`,
keyring service `claude-mini`, Rust crate name `claude-mini`. Only
user-facing strings changed. Export folder became "Documents/Mini Chat
for Claude" (old exports stay in "Claude Mini" — harmless).

**Done this session:**
- Version 0.2.0 everywhere (tauri.conf.json, Cargo.toml, package.json).
- CSP tightened from `null` to
  `default-src 'self'; connect-src ipc: http://ipc.localhost;
  style-src 'self' 'unsafe-inline'; img-src 'self' data:`
  ⚠️ needs a visual smoke-test — CSP breakage is silent.
- LICENSE (MIT), public README (unofficial disclaimer + trademark note,
  BYO-API-key explanation, SmartScreen instructions, screenshot TODO).
- **Auto-updater wired end-to-end:** tauri-plugin-updater; startup check
  in lib.rs emits `update-available` → JS banner above composer → click
  → `install_update` command (download_and_install + restart). Signing
  keypair generated: private key at `C:\Users\brett\.tauri\
  mini-for-claude.key` (NEVER commit; no password), pubkey embedded in
  tauri.conf.json. `createUpdaterArtifacts: true`. Endpoint points at
  GitHub Releases `latest.json` — **placeholder `YOUR-GITHUB-USERNAME`
  needs Brett's real username** (updater fails silently until then).
- **CI:** `.github/workflows/release.yml` — tauri-action on `v*` tags,
  windows-latest, drafts a GitHub Release with the installer + updater
  artifacts. Needs repo secrets `TAURI_SIGNING_PRIVATE_KEY` (key file
  contents) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (empty string).
- `.gitignore`: Cargo.lock now COMMITTED (reproducible CI builds).
- gh CLI not installed → repo created/pushed manually by Brett (runbook
  below).

**v0.2.0 built + signing verified 2026-07-02 14:40:** installer (1.95 MB)
+ `.sig` updater signature both produced, exit 0. Local-build gotcha:
`TAURI_SIGNING_PRIVATE_KEY_PATH` did NOT work through the npm chain here —
pass the key CONTENTS instead:
`TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/mini-for-claude.key)"
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" npm run tauri build`
(This mirrors the CI secret exactly, so the workflow config is
rehearsal-validated.)

**Publish runbook (Brett's part, ~10 min):**
1. github.com → New repository → name `mini-chat-for-claude`, public,
   no README/license (we have them) → Create.
2. Tell Claude the GitHub username → endpoint placeholder gets fixed +
   committed.
3. In the project dir:
   `git remote add origin https://github.com/<user>/mini-chat-for-claude.git`
   `git push -u origin main` (browser auth popup on first push).
4. Repo Settings → Secrets and variables → Actions → add
   `TAURI_SIGNING_PRIVATE_KEY` (paste contents of the .key file) and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (empty).
5. Screenshot/GIF into README (the TODO marker), commit.
6. `git tag v0.2.0 && git push --tags` → CI builds → draft release
   appears → write notes → Publish. World: shipped.
7. **winget** (after release is live): PR a manifest to
   microsoft/winget-pkgs (`wingetcreate new <installer-url>` automates
   it). Package id suggestion: `BrettCherry.MiniChatForClaude`.
8. **Store later:** $19 dev account → we add MSIX target + Store listing;
   identifier/branding already compatible.

**Brett's Option B vision (recorded for v0.3+):**
Interested in the Claude Agent SDK path (Pro subscription powering personal
tools, no API credits) — but bigger than that: *"an app, or apps, that can
run in as many separate applications as possible to make it accessible."*
Claude as an ambient layer across surfaces, not one widget. Ideas parked
here: subscription-mode variant via Agent SDK, tray/menubar companion,
browser-side presence, share-target integrations. Explore after v0.1 ships.

### Session 2026-06-12 — REAL root cause found: the window was never shown

Picked back up after a few weeks. The "window doesn't appear" saga is over,
and the actual root cause was not what we thought.

**The revelation:** `EnumWindows` showed the real Tauri window sitting at a
sane position/size with **visible=False**. Despite `visible: true` in
tauri.conf.json, the frameless+transparent window is created but never
shown on this machine. Every earlier "fix" worked by accident — the Win32
`SetWindowPos` calls included `SWP_SHOWWINDOW` (0x0040), so it was the
*showing*, not the moving, that made the window appear. Earlier
"offscreen" readings came from .NET's `MainWindowHandle` heuristic
grabbing helper windows (e.g. Tao's 16×16 "Thread Event Target"), not the
real window. Lesson: enumerate windows by class (`Tauri Window`), never
trust `MainWindowHandle` for Tauri apps.

**The fix (verified working — VISIBLE=True at exactly (1250,620)):**
1. `window.rs` calls `window.show()` + `set_focus()` unconditionally at
   the end of setup.
2. The window-state plugin is excluded from managing visibility
   (`StateFlags::all() & !StateFlags::VISIBLE`) so it can never
   reintroduce the problem.
3. First-launch placement is still done manually (explicit
   `set_size(LogicalSize)`, then centered `set_position` computed from
   monitor geometry + scale factor) — diagnostics confirmed the math:
   monitor 2880×1800 @ scale 1.0 → window (1250,620) 380×560. ✓
4. `eprintln!` diagnostics stay in `place_first_launch` — dev-only
   stderr, invaluable for this class of bug.

**Same session, part 2 — the ✕ trap:**
- Brett hid the window with the ✕ button minutes into real use, and v0.1
  had no way to bring it back (the known "no tray yet" gap). External
  `ShowWindow` from PowerShell doesn't stick — Tauri re-asserts its
  internal hidden state — so the only recovery was restarting the app.
- Fix: added `tauri-plugin-global-shortcut`, registered
  **Ctrl+Shift+Space** as a system-wide summon/dismiss toggle in `lib.rs`.
  Registration failure (shortcut taken by another app) logs to stderr but
  doesn't crash. Welcome screen + ✕ tooltip now advertise the shortcut.
- Verified end of session: window VISIBLE=True, shortcut registered with
  no errors, and Brett had already dragged the window to his preferred
  spot (lower-left) — which the window-state plugin will persist.

**Model IDs verified (caveat from last session resolved):**
- Old guesses (`claude-opus-4-7-latest` etc.) were wrong — the `-latest`
  suffix convention didn't apply, and Opus is at 4.8 now.
- Current lineup in the picker: Fable 5 (`claude-fable-5`),
  Opus 4.8 (`claude-opus-4-8`), Sonnet 4.6 (`claude-sonnet-4-6`),
  Haiku 4.5 (`claude-haiku-4-5-20251001`). Default: Sonnet 4.6.
- Still TODO someday: populate the picker from `/v1/models` instead of
  hardcoding.

**Still pending:** the end-to-end streaming test. Needs
`$env:ANTHROPIC_API_KEY = 'sk-ant-...'` set in the shell before
`npm run tauri dev`.

### v0.1 milestone 2 — streaming + window persistence — 2026-05-26

**Anthropic streaming wired end-to-end:**
- Rust: `anthropic.rs` is a tiny SSE parser using `reqwest` + `futures-util`.
  Parses `data: {...}\n\n` blocks, surfaces `content_block_delta` text and
  `message_stop` via a callback.
- Tauri command `send_chat` reads the API key from `ANTHROPIC_API_KEY` env
  var (v0.2 will replace with `keyring`), invokes the stream, emits each
  delta as a `chat-chunk` event back to the frontend.
- Frontend: a streaming bubble accumulates tokens live with a blinking
  caret. History is JS-owned (`[{ role, content }]`), passed fully on each
  turn (stateless command). Composer disables while a request is in flight.

**Window state persistence:**
- Added `tauri-plugin-window-state` — position, size, maximized state all
  saved to `<app_config>/.window-state.json` on close, restored on next
  launch.
- `window.rs` now only calls `.center()` if no saved state file exists.
  This preserves the multi-monitor fix from milestone 1 *and* respects
  the user's last placement.

**Env var requirement (v0.1 only):**
- Before `npm run tauri dev`, set `ANTHROPIC_API_KEY` in the shell.
- PowerShell: `$env:ANTHROPIC_API_KEY = 'sk-ant-...'`
- v0.2 will replace this with a proper first-run UX + keychain.

### v0.1 scaffold landed — 2026-05-26 🎉

First launch worked. The window popped open looking exactly like the spec:
frameless, dark, rounded, 380×560, drag strip at top, composer at bottom, all
container-query breakpoints behaving when resized.

**Bug we hit + fixed:**
- The `center: true` config flag silently failed — window opened at
  `(L=2284, T=659)` and was offscreen for Brett. Process was alive, window
  existed in the OS window list, just rendering on a phantom monitor.
- Worked around it live by `SetWindowPos`-ing the window onto the primary
  display via PowerShell + Win32 API.
- Permanent fix: call `window.center()` programmatically inside
  `window::apply_platform_chrome()` instead of relying on the config flag.
- Lesson: anything position/size related on Windows belongs in Rust code,
  not config — config flags are best-effort, runtime calls are deterministic.

**What's verified working:**
- Frameless transparent window with rounded corners + backdrop-filter blur
- Drag region (grab the top strip to move the window)
- Pin button toggles always-on-top via Tauri IPC
- Close button hides the window
- Resize from any edge, min/max bounds enforced (280×320 → 800×900)
- Container queries reflow the UI as window shrinks (model badge + titlebar text vanish under 360px)
- Composer auto-grows up to 160px, Enter sends, Shift+Enter newlines
- Stub echo confirms full UI round-trip

### Decisions locked 2026-05-26
- **Stack:** Tauri 2 (Rust + system webview)
- **Frontend:** Vanilla HTML/CSS/JS + Vite (no framework for v0.1 — keep dependencies minimal, matches "tiny widget" ethos; can adopt Svelte/Lit later if state management gets gnarly)
- **v0.1 target:** Windows only
- **v0.2 target:** Linux port (Mac on hold — no Mac hardware)
- **Strategy:** Isolate platform-specific code (window chrome, blur effects, tray) into one Rust module from day one, so Linux is a small additive change, not a refactor.

---

## File structure

```
cldMiniApp/
├── PLAN.md                       ← this file
├── README.md
├── .gitignore
├── package.json                  ← frontend tooling (Vite)
├── vite.config.js
├── index.html                    ← frontend entry
├── src/                          ← frontend (vanilla JS)
│   ├── main.js                   ← app bootstrap
│   ├── styles.css                ← container-query responsive layout
│   ├── chat.js                   ← message state + rendering
│   ├── api.js                    ← calls into Rust backend via invoke()
│   └── components/
│       ├── messageList.js
│       ├── inputBar.js
│       └── modelPicker.js
└── src-tauri/                    ← Rust backend
    ├── Cargo.toml
    ├── tauri.conf.json           ← window + bundle config
    ├── build.rs
    ├── icons/                    ← auto-generated by tauri icon
    ├── capabilities/
    │   └── default.json          ← Tauri 2 permission grants
    └── src/
        ├── main.rs               ← thin entry — calls lib::run()
        ├── lib.rs                ← Tauri app setup, command registration
        ├── window.rs             ← platform-specific window chrome (cfg-gated)
        ├── commands.rs           ← Tauri commands exposed to JS
        ├── anthropic.rs          ← Anthropic streaming client (reqwest + SSE)
        └── secrets.rs            ← API key storage via keyring crate
```

### Why this split

- **`src-tauri/src/window.rs`** is where every `#[cfg(target_os = "windows")]` and `#[cfg(target_os = "linux")]` block lives. If a platform decision creeps into another file, that's the smell that tells us to refactor.
- **`anthropic.rs` lives in Rust, not JS.** Reason: streaming with SSE is cleaner with reqwest's response stream, and the API key never has to cross the Tauri IPC boundary in plaintext.
- **`secrets.rs` wraps `keyring`** so both Windows Credential Manager and Linux libsecret are abstracted behind one tiny interface.
- **No state management library in JS.** A ~200-line `chat.js` with an event emitter is plenty for v0.1.

---

## Initial `tauri.conf.json`

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Claude Mini",
  "version": "0.1.0",
  "identifier": "com.brettcherry.claudemini",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "Claude Mini",
        "width": 380,
        "height": 560,
        "minWidth": 280,
        "minHeight": 320,
        "maxWidth": 800,
        "maxHeight": 900,
        "resizable": true,
        "decorations": false,
        "transparent": true,
        "alwaysOnTop": false,
        "skipTaskbar": false,
        "shadow": true,
        "center": true,
        "visible": true
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

### Key window choices, justified

- `decorations: false` — no OS title bar; we draw our own drag region in CSS
- `transparent: true` — so we can do rounded corners and (later) acrylic/Mica blur
- `alwaysOnTop: false` (default) — but exposed as a runtime toggle from the UI
- `width: 380, height: 560` — Spotify mini player is ~350×570 by default; we're in the same ballpark
- `min: 280×320` — below this the layout would have to abandon the message bubbles entirely (saved for a future "ultra-compact" mode)
- `max: 800×900` — wide enough for comfortable reading without becoming a "real app"
- `shadow: true` — drop shadow around the frameless window so it doesn't look pasted onto the desktop
- `csp: null` — disabled for dev; we'll tighten this before shipping a real release
