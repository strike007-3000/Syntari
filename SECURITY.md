# Security Policy

## Supported versions

Syntari is pre-1.0 and has no auto-updater. Only the latest [GitHub Release](https://github.com/strike007-3000/Syntari/releases) and `main` receive fixes.

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's private reporting instead:
**[Security tab → Report a vulnerability](https://github.com/strike007-3000/Syntari/security/advisories/new)**.

Include the affected version (shown in the app header), your OS, reproduction steps and the impact you see. This is a small project: expect an acknowledgement within about a week, and a fix or mitigation plan after triage. Credit is given in the release notes unless you prefer otherwise.

## What is in scope

Syntari is a local-only app, so the issues that matter most are those that break its privacy promises:

- Audio, transcripts, notes or keystrokes leaving the device (anything beyond the one-time model download).
- Plaintext audio or transcripts persisting where the README says they do not (the temp `.wav` staging, the log file, the database).
- Weakening of the Tauri CSP or capability set (`src-tauri/tauri.conf.json`, `src-tauri/capabilities/`).
- Model download or integrity-check bypasses (SHA-256 verification, `SYNTARI_MODELS_DIR` handling).
- Path traversal or arbitrary file write through export, backup or log features.

## Known and accepted limitations

These are documented in the [README](README.md) (sections 5 and 8) and are not vulnerabilities by themselves:

- The database and `syntari.log` are not encrypted at rest. Use FileVault (macOS) or BitLocker (Windows).
- The closed-source Foundry Local runtime may make its own network requests. Check with `scripts/check-egress.sh` (macOS/Unix).
- Model integrity is trust-on-first-use, not pinned to upstream hashes.
- Release builds are unsigned (macOS ad-hoc signed only; Windows not signed). Verify downloads come from this repository's Releases page.
- Output of the on-device models can be wrong; it is not a security boundary.

Vulnerabilities in dependencies (Tauri, Foundry Local, ONNX Runtime, cpal and others) should be reported to those projects; tell us too if Syntari ships an affected version.
