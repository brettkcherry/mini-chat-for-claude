# Security Policy

## Reporting a vulnerability

Please **do not open a public issue** for a security problem — this app
stores a live Anthropic API key in Windows Credential Manager, and a public
report gives an attacker a head start before a fix ships.

Instead, use GitHub's private vulnerability reporting:

**[Report a vulnerability →](https://github.com/brettkcherry/mini-chat-for-claude/security/advisories/new)**

This opens a draft security advisory visible only to the maintainer. You'll
get a response, and credit in the eventual advisory if you'd like it.

If you'd rather not use GitHub for the initial report, open a regular issue
asking to be contacted privately — don't include exploit details in it.

## Supported versions

This project ships one rolling release. Only the latest tagged version on
[Releases](https://github.com/brettkcherry/mini-chat-for-claude/releases)
receives fixes; there's no long-term-support branch to report against.

## Scope

In scope: the desktop app itself (`src-tauri/`, `src/`), the release
workflow (`.github/workflows/`), and how the installer/uninstaller handle
the stored API key and session data.

Out of scope: Anthropic's API and infrastructure (report those to
Anthropic directly), and vulnerabilities in a third-party dependency that
don't have an app-specific exploitation path here — file those upstream,
though a heads-up here is still welcome so the dependency can be bumped.

## What "vulnerability" means here

A non-exhaustive list of things worth a private report rather than a public
issue: a way to read or exfiltrate the stored API key, a way to run
arbitrary code via a crafted model response (prompt injection into the
sanitizer, a Tauri IPC bypass), a path-traversal or file-write bug in
session handling, or a supply-chain issue in the release pipeline (e.g. an
unpinned action, a way to tamper with a published update artifact).

Ordinary bugs, UI issues, and feature requests belong in the regular
[issue tracker](https://github.com/brettkcherry/mini-chat-for-claude/issues).

## Properties this app intends to hold

- **The API key never enters the webview.** It is read in Rust and used there;
  the frontend can ask whether a key exists and see its last four characters,
  and that is all. `ANTHROPIC_API_KEY` is honoured only in debug builds — in a
  shipped app an environment variable is an injection point, not a
  convenience.
- **Model output cannot become markup.** Responses go through DOMPurify before
  they reach `innerHTML`, and everything interpolated into the sessions list is
  escaped. A stale DOMPurify is therefore a live exposure, not routine drift,
  which is why Dependabot watches it.
- **The frontend cannot choose a path on disk.** Exporting a transcript hands
  Rust a title and the contents; Rust runs the save dialog and writes only to
  what the user picked there. This replaced an earlier design where the
  frontend passed a path to a `fs::write` — the dialog was a convention rather
  than a boundary, so anything controlling the webview could have written
  anywhere the user could.

## Automated gates

These run on every push and pull request, because a check that has to be
remembered is a check that eventually isn't:

| Gate | What it catches |
|---|---|
| `cargo test` | Regressions in session handling, key storage, model parsing |
| `npm audit --omit=dev` | Advisories in dependencies that actually ship |
| `rustsec/audit-check` | Advisories in the Rust tree, from a different data source |
| `cargo deny check licenses` | A dependency arriving under terms MIT can't ship |
| `npm ci --ignore-scripts` | Install-hook payloads, including in the job holding the signing key |

Release workflow actions are pinned to full commit SHAs. That job holds the
key authorising updates to every installed copy, so a retagged action would be
a direct path to shipping a malicious update.
