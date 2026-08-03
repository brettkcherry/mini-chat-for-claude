# LAUNCH.md — public-release readiness review

**Reviewed:** 2026-08-03, against `main` @ `dcc1e6b` (v0.3.0 published).
**Scope:** everything a stranger touches — the binary, the install, the repo,
the release page, the legal surface.

> Read [PLAN.md](./PLAN.md) first for project history. This file is the
> forward-looking checklist; PLAN.md is the log. When an item here is done,
> tick it here and record the *why* there.

---

## Verdict

**The engineering is ready. The presentation and three streaming bugs are not.**

This is unusually well-prepared for a solo project. Supply chain is pinned and
audited, secrets live in the OS credential store with a working uninstall path,
CSP is real, the updater is signed and verified end-to-end on a live install,
and the test bench covers exactly the pure logic worth covering. Almost nothing
on a normal "getting ready to ship" list is outstanding.

What stands between here and a launch you'd actively promote:

1. **Three defects in the streaming path** that all present as *lost or stuck
   replies* and all look random to the user. These are the kind of bug that
   generates "it just stops sometimes" issues you can't reproduce. Fix before
   an audience arrives, not after.
2. **The app has no face.** No screenshot in the README, empty release notes on
   all three releases, and an icon that Brett himself flagged as trademark
   exposure and never replaced. A floating-widget app is sold visually; right
   now there is nothing to look at.
3. **Nothing enforces the CI that exists** — no branch protection — and the
   updater signing key exists on exactly one machine with no documented backup.

None of this is deep. Realistically: one focused session on the bugs, one on
presentation, one on repo hygiene, then tag and go.

---

## Scorecard

| Area | State | Notes |
|---|---|---|
| Security posture | ✅ Strong | Keychain, CSP, SHA-pinned actions, npm+cargo audit in CI, private-report policy |
| Supply chain | 🟡 Good, drifting | 15 open Dependabot PRs, 4 of them majors |
| Licensing / attribution | ✅ Done | MIT, NOTICE.md, 547-crate license bundle, bundle metadata |
| Uninstall / data hygiene | ✅ Done | NSIS hook clears the credential, update-mode guarded |
| Update pipeline | ✅ Proven live | Signed, verified, demonstrated 0.2.0 → 0.2.1 on a real install |
| Test coverage | 🟡 Backend only | 26/26 green as of this review; installer/tray/updater/themes are manual and unchecklisted |
| **Streaming correctness** | ❌ **Three real bugs** | See B1–B3 |
| Presentation | ❌ Not started | No screenshot, no release notes, no changelog |
| Brand / trademark | 🟡 Unresolved | Icon still the lowercase-c in Anthropic's orange |
| Repo governance | 🟡 Thin | No branch protection, no issue templates, no topics |
| Code signing | 🟡 Deliberate gap | Unsigned by choice; documented; SignPath unexplored |
| Key custody | ❌ Single point of failure | Updater private key on one machine, no backup noted |
| Accessibility | 🟡 Unaudited | No `aria-label`s, no `aria-live` on the transcript |

---

## Blockers — fix before promoting this anywhere

### B1. A split UTF-8 character kills the whole reply
`src-tauri/src/anthropic.rs:165`

```rust
let text = std::str::from_utf8(&chunk).map_err(|e| format!("utf8 decode error: {e}"))?;
```

`res.bytes_stream()` yields arbitrary byte chunks. A multi-byte character — em
dash, curly quote, emoji, any accented name — that happens to straddle a chunk
boundary makes this `Err`, and the `?` aborts the entire stream.

It gets worse on the frontend: `submit()`'s `catch` does
`streamingBubble.textContent = String(err)` (`src/main.js:387`), so the partial
reply the user was reading is **replaced** by `utf8 decode error: ...`.
Everything streamed so far is gone, and nothing reaches `history`.

Intermittent, input-dependent, and impossible for a user to describe usefully.

**Fix:** accumulate into a `Vec<u8>`; decode the longest valid prefix each
round and carry the incomplete tail forward. Same pattern `SseParser` already
uses for event boundaries, one layer down. Add a test that pushes a two-byte
character split across two `push` calls.

### B2. A mid-stream API error orphans the bubble and loses the turn
`src-tauri/src/anthropic.rs:90-111`, `src/main.js:440-460`

The parser dispatches on `content_block_delta` and `message_stop` and drops
everything else. Anthropic also emits `error` events mid-stream
(`overloaded_error`, `api_error`) after content has already flowed.

When that happens the stream just ends. `stream_chat` returns `Ok(())`,
`send_chat` returns clean, and the frontend's `finally` re-enables the composer
— but the `stop` branch never runs, so:

- `streamingRaw` is **never pushed to `history`** → the reply is on screen but
  invisible to the next turn's context
- `.msg--streaming` is never removed → the blinking caret runs forever
- `streamingBubble` stays non-null

The same thing happens on any clean connection close without `message_stop`.

**Fix (both ends):** handle `type: "error"` in `SseParser` and surface it as a
terminal event; and make the frontend treat "`send_chat` resolved while a
bubble is still streaming" as an implicit stop — commit what arrived, drop the
streaming class, append a small "the response ended early" note.

### B3. `max_tokens: 4096` truncates silently
`src-tauri/src/anthropic.rs:135`

Hardcoded, and `message_delta`'s `stop_reason: "max_tokens"` is among the event
types the parser ignores. A long answer stops mid-sentence with no explanation
and no visual difference from a normal ending.

The picker now offers Opus 5 at `max` effort. 4096 is not a plausible ceiling
for that.

**Fix:** raise it (per-model if the `/v1/models` payload carries a max — it's
already being parsed for effort levels), and read `stop_reason` so a truncated
reply can say so.

### B4. Back up the updater signing key
`~/.tauri/mini-for-claude.key`, one machine, no backup recorded anywhere.

Every installed copy of the app verifies updates against the public key baked
into `tauri.conf.json`. If the private key is lost, **auto-update is
permanently dead for every existing install** — there is no rotation path that
reaches an already-installed app. Recovery means telling users to manually
download a new installer built with a new key.

The risk goes up the moment there are users who aren't Brett.

**Fix:** copy it into a password manager (and one offline location), and note
in PLAN.md where it lives. Five minutes, removes an unrecoverable failure mode.

### B5. Empty release notes on all three releases
`v0.2.0`, `v0.2.1`, `v0.3.0` all have an empty body. v0.3.0 is what
`/releases/latest` shows a stranger.

An unsigned installer downloaded from a release page with no notes and no
checksums is a hard sell. The notes are also where the SmartScreen warning
should be explained *at the point of download*, not only in the README.

**Fix:** write notes for v0.3.0 (a draft with checksums was written to a
scratchpad in the 2026-08-02 session — regenerate if lost), include SHA-256 of
the installer, and backfill one-liners on 0.2.0/0.2.1.

### B6. No screenshot in the README
`README.md:9` — still `<!-- TODO -->`. PLAN.md's own `next:` field has said so
since v0.2.

This is the highest-leverage single item in this file. A frameless floating
widget cannot be sold in prose.

**Fix:** one still (dark theme, mid-conversation, on a real desktop so the
frameless edge and shadow read) plus one short GIF (summon via
`Ctrl+Shift+Space` → type → stream → dismiss). Resize the window on camera if
the GIF has room — the container-query reflow is the differentiator and nobody
will discover it from text.

### B7. Icon still carries the trademark exposure Brett flagged
`src-tauri/icons/` is unchanged since 2026-07-03 — the lowercase-`c` in
Anthropic's orange-rust palette. `icon-drafts/` has iterations through v6
(`o_`/`p_`) with no final selection.

Brett's own reasoning from the 2026-07-31 session still stands: the icon is
what people recognize, and the README disclaimer doesn't cover trade dress.
Shipping it quietly to a handful of users is one risk posture; putting it on
winget and an announcement post is another.

**Fix:** pick a draft, run the taskbar-size legibility pass, `npm run tauri
icon`, commit. Blocks W2, not the bug-fix release.

---

## Should-fix — not blocking, but they shape the first impression

### S1. No way to stop a response in flight
There is no cancel. Composer is disabled for the whole turn, and on a
max-effort Opus reply that's a long time to be locked out of your own widget —
while being billed. A stop button that swaps in for send, backed by a
cancellation token in `stream_chat`, is the single most-missed chat affordance.

### S2. Conversation history grows without limit or warning
`main.js:376` sends `history.slice()` — the whole thing, every turn. No
trimming, no token counter, no warning. A long session eventually 400s on
context length, and the raw API error string is what the user sees.

Minimum viable: detect that error and say "this conversation is too long —
start a new chat" in plain words. Better: a subtle turn/token indicator once a
session gets long.

### S3. Accessibility is unaudited
Titlebar buttons carry `title` but no `aria-label`; the transcript has no
`aria-live` so a screen reader never announces a streamed reply; focus
visibility hasn't been checked against either theme. All cheap now, all
awkward once someone files an issue.

### S4. Dependency triage — 15 open PRs, 4 needing judgment
Routine, but not all auto-mergeable:

| PR | Bump | Care needed |
|---|---|---|
| #5 | `tauri-action` 0.6.2 → **1.0.0** | Touches the job holding the signing key. Read the changelog; test on a throwaway tag. |
| #4 | `actions/checkout` 4 → **7** | Major; verify the `# v4` trailing comment gets rewritten to match |
| #7 | `actions/setup-node` 4 → **7** | Same |
| #12 | `vite` 6 → **8** | Build-time only, but rebuild + smoke-test the bundle |
| #11 | `quinn-proto` | **Check first:** if Dependabot resolves this without the `rand` 0.9→0.10 cascade, merge it and delete the `--ignore RUSTSEC-2026-0185` line from `test.yml` |
| rest | patch/minor | Batch them |

Do the minors first so the majors land against a clean tree.

### S5. No branch protection on `main`
Three CI jobs exist and nothing requires them. Push straight to `main` still
works. Once anyone else can open a PR, require the three checks and disallow
direct pushes.

### S6. Repo governance gaps
- No `CHANGELOG.md` — releases are the only history, and their bodies are empty
- No issue templates — a bug template asking for **app version, Windows build,
  and whether it survives a restart** will pay for itself on the first report
- No `CONTRIBUTING.md` — even three lines ("run `npm run tauri dev`, `cargo
  test` must pass, no new dependencies without a reason") sets expectations
- No repo topics → invisible to GitHub search. Add: `tauri`, `rust`, `claude`,
  `anthropic`, `windows`, `desktop-widget`, `chat`
- Wiki is on and empty; Discussions is off. Flip both.
- `security-hardening` branch is merged and stale — delete it

### S7. In-app disclaimer
The "unofficial, not affiliated with Anthropic" notice lives only in the README.
The installed app never says it. Put one line in the Settings footer next to
the version string — that's the surface a user actually sees, and it's where
the claim matters.

### S8. Cost transparency
BYO-key means the user pays per token, and the effort picker silently changes
what a turn costs. The README explains the key is separate from a Claude.ai
subscription (good) but never says "you are billed per message." Say it.

### S9. Formalize the privacy statement
`README.md:7` already lists all three endpoints accurately and it's genuinely
better than most privacy policies. Promote it to `PRIVACY.md`, add the explicit
negatives — **no telemetry, no analytics, no crash reporting, no account** —
and link it from Settings. That's a selling point, not boilerplate.

### S10. Pin the WebView2 install mode
`bundle.windows.webviewInstallMode` isn't set, so Tauri's default
(`downloadBootstrapper`) applies — which needs a live network connection during
install. Fine, but make it explicit in the config and mention it in the
README's Win10 line so the behaviour is a decision rather than a default.

### S11. No manual pre-release smoke checklist
TESTING.md covers the unit bench honestly and explains what it deliberately
skips. But everything CI can't touch is also the riskiest surface: installer,
uninstaller checkbox, tray, updater, themes, first-run. A written checklist
(draft in W4) turns "I think I tested that" into a signed-off list.

---

## Waypoints

Sequential. Each gate ends in something you can point at.

### W0 — Ten minutes, do it now
- [ ] Back up `~/.tauri/mini-for-claude.key` to a password manager + one
      offline location; record where in PLAN.md **(B4)**
- [ ] `git branch -d security-hardening` and delete it on origin **(S6)**
- [ ] Add repo topics, enable Discussions, disable the empty Wiki **(S6)**

### W1 — Correctness gate → ship **v0.3.1**
No audience should meet B1–B3.

- [ ] Fix the split-UTF-8 decode; add a split-character regression test **(B1)**
- [ ] Handle `error` events + implicit stop on both ends **(B2)**
- [ ] Raise `max_tokens`; surface `stop_reason: "max_tokens"` **(B3)**
- [ ] Add the stop button + cancellation token **(S1)** *(optional here, but
      it's the same file and the same test pass)*
- [ ] Friendly message on context-length 400 **(S2)**
- [ ] `cargo test` green; manual smoke on a long reply with emoji in it
- [ ] Tag `v0.3.1`, write real notes with checksums, publish
- [ ] Confirm the live 0.3.0 install auto-updates to it

### W2 — Identity gate
The app gets a face.

- [ ] Pick the final icon from `icon-drafts/`, taskbar legibility pass,
      `npm run tauri icon`, commit **(B7)**
- [ ] README screenshot + GIF **(B6)**
- [ ] In-app "unofficial" line in Settings **(S7)**
- [ ] Cost-transparency line in the README **(S8)**
- [ ] `PRIVACY.md`, linked from Settings **(S9)**
- [ ] `aria-label`s + `aria-live` on the transcript; tab through both themes **(S3)**

### W3 — Repo hygiene gate
What a contributor and a security researcher land on.

- [ ] Merge the patch/minor Dependabot PRs as a batch **(S4)**
- [ ] Handle the four majors one at a time, `tauri-action` last **(S4)**
- [ ] Resolve or re-document `quinn-proto`; drop the `--ignore` if #11 clears it **(S4)**
- [ ] Branch protection on `main`: require the three checks, no direct pushes **(S5)**
- [ ] `CHANGELOG.md`, backfilled to 0.2.0 **(S6)**
- [ ] Issue templates (bug / feature) + three-line `CONTRIBUTING.md` **(S6)**
- [ ] Backfill notes on the 0.2.0 / 0.2.1 releases **(B5)**
- [ ] Pin `webviewInstallMode` **(S10)**

### W4 — Release gate → **v0.4.0**
- [ ] Write the manual smoke checklist into TESTING.md **(S11)**:
      - clean install on a machine with no prior version
      - first run with no key → composer disabled, key card pinned
      - save a key → status shows the `…abcd` suffix
      - stream a long reply containing emoji and a code block
      - `Ctrl+Shift+Space` hide/summon; ✕ to tray; tray click restores;
        second launch attempt restores rather than duplicating
      - light / dark / system, plus the opacity slider in both
      - resize from 280 to 800 px, watch all three breakpoints
      - export .md, delete one session, delete-all with the confirm step
      - uninstall **without** the checkbox → key survives
      - uninstall **with** the checkbox → `cmdkey /list` shows nothing
      - auto-update from the previous version on a real install
- [ ] Run it, top to bottom, on the shipped build
- [ ] Tag, notes, checksums, publish
- [ ] Verify auto-update once more

### W5 — Go public
- [ ] `wingetcreate new <installer-url>` → PR to microsoft/winget-pkgs as
      `BrettCherry.MiniChatForClaude`; expect manifest review round-trips
- [ ] Announce (r/tauri, r/ClaudeAI, Hacker News "Show HN"). Lead with the
      GIF and the 1.8 MB installer size. Say "unofficial" and "bring your own
      API key" in the first two sentences — both are trust signals, not
      caveats.
- [ ] Watch the first 48 hours: issues, Dependabot, and whether SmartScreen
      reputation starts accumulating

### Parallel track — code signing (not blocking)
- [ ] Retry `signpath.io` (the 2026-08-02 attempt failed on a header-parsing
      error, not a rejection). Their OSS program is the only realistic $0 path
      — Azure Artifact Signing was ruled out: US/Canada only, no free tier,
      quote-only pricing.
- [ ] If it lands: add the signing step to `release.yml` and drop the
      SmartScreen paragraphs from the README and release notes.
- [ ] If it doesn't: unsigned is a legitimate posture for a solo OSS dev.
      Publishing SHA-256 checksums with every release is the compensating
      control, and it's already in W1.

---

## Post-launch operations

Worth deciding *before* strangers arrive, not during:

- **Issue response.** Set an expectation you'll actually meet. "Best-effort,
  usually within a week" beats silence.
- **Dependabot cadence.** Weekly batch, or it becomes 15 PRs again.
- **Release cadence.** Rolling-latest is already the stated policy in
  SECURITY.md. Keep it — don't accidentally start maintaining branches.
- **The updater is a live channel into every install.** The signing key and the
  `TAURI_SIGNING_PRIVATE_KEY` secret are, jointly, the crown jewels. Nothing
  else in this repo deserves that level of care.
- **Scope discipline.** The first popular request will be "add tabs" or "add
  MCP". PLAN.md already argues against tabs. Decide the boundary in advance so
  the answer is a policy rather than a mood.

---

## Explicitly not doing (and that's correct)

- **Linux port** — planned as v0.2, still unbuilt. Windows-only is a clear,
  honest scope. Say it plainly at the top of the README and it stops being a
  gap.
- **Frontend test bench** — TESTING.md's reasoning is sound: `main.js` is DOM
  wiring, and the pure logic worth testing is already in Rust.
- **`glib` unsoundness advisory** — cargo-audit itself buckets it as a warning,
  and it isn't actionable without dropping the GTK tray dependency.
- **Telemetry** — its absence is a feature. Say so in PRIVACY.md rather than
  quietly reconsidering it later.
