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
