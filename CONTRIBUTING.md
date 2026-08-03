# Contributing

Thanks for looking. This is a small, deliberately narrow app — a floating chat
widget, not a Claude client with everything in it. That scope is the main thing
to know before opening a PR.

## Getting it running

Prerequisites: Rust (stable) and Node 18+.

```bash
npm install
npm run tauri dev
```

You'll need an Anthropic API key with credits. In a dev build you can export
`ANTHROPIC_API_KEY` instead of using the key card — that fallback is compiled
out of release builds on purpose.

## Before you open a PR

- `cargo test --manifest-path src-tauri/Cargo.toml` must pass. CI runs it on
  Windows, along with `npm audit` and `cargo-audit`.
- New logic worth testing goes in a pure function that can be driven without a
  live network connection or a Tauri `AppHandle` — see [TESTING.md](./TESTING.md)
  for why the code is factored that way.
- New dependencies need a reason. The pitch for this app is a 1.8 MB installer;
  every crate and package is also something to audit and keep patched.
- Platform-specific code belongs in `src-tauri/src/window.rs`. If a
  `#[cfg(target_os = ...)]` block appears anywhere else, that's the signal to
  refactor.

## Scope

Things that are unlikely to be merged, so you don't spend an evening on them:

- Multi-conversation tabs. This is a widget that sits next to your work, and
  tabs turn it into an app you switch to.
- Anything that sends data anywhere other than Anthropic's API. The three
  endpoints this app talks to are documented in the README, and that list
  should stay short enough to print.
- Telemetry or analytics of any kind.

Things very much wanted: bug reports with reproduction steps, accessibility
fixes, and the Linux port (`window.rs` and the `keyring` feature flags are the
two places that need attention — see the notes in `Cargo.toml`).

## Reporting bugs

Use the issue templates. The three things that make a report actionable are the
app version (Settings shows it), your Windows build, and whether it survives a
restart.

Security problems go through [SECURITY.md](./SECURITY.md) instead — please
don't open a public issue for anything that could expose someone's API key.
