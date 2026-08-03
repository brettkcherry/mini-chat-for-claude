# Contributing

Thanks for looking. This is a deliberately narrow app — a floating chat widget,
not a Claude client with everything in it. That scope is the main thing to know
before opening a PR.

## Running it

Rust (stable) and Node 18+.

```bash
npm install
npm run tauri dev
```

You'll need an Anthropic API key with credits. In a dev build you can export
`ANTHROPIC_API_KEY` instead of using the key card — that fallback is compiled
out of release builds on purpose.

## Before a PR

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Must pass. CI runs it on Windows alongside `npm audit` and `cargo-audit`.

A few things that will come up in review, so they're worth knowing up front:

- **Testable logic goes in pure functions.** The bench covers string and byte
  processing with no I/O — SSE parsing, session-ID validation, filename
  sanitising — because those can be driven with plain values instead of a live
  connection or a Tauri `AppHandle`. If a change needs a test, the usual move is
  to extract the pure part and have the original call it.
- **Platform-specific code belongs in `src-tauri/src/window.rs`.** A
  `#[cfg(target_os = ...)]` anywhere else is the signal to refactor.
- **New dependencies need a reason.** The pitch is a ~2 MB installer, and every
  package is also something to audit and keep patched.
- **Nothing about a model gets hardcoded.** Model IDs, effort levels and token
  ceilings come from `/v1/models`. Every baked-in version of that list has
  rotted within weeks.

## Scope

Unlikely to be merged, so you don't spend an evening on them:

- **Multi-conversation tabs.** This is a widget that sits beside your work;
  tabs turn it into an app you switch to.
- **Any new network endpoint.** The app talks to three hosts, all listed in the
  README, and that list should stay short enough to print.
- **Telemetry or analytics of any kind.**

Very much wanted: bug reports with reproduction steps, accessibility fixes, and
the Linux port — `window.rs` and the `keyring` feature flags in `Cargo.toml` are
the two places that need attention.

## Reporting bugs

Use the issue templates. The three things that make a report actionable are the
app version (Settings shows it), your Windows build, and whether it survives a
restart.

Security problems go through [SECURITY.md](./SECURITY.md) instead — please don't
open a public issue for anything that could expose someone's API key.
