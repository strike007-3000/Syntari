# Contributing to Syntari

Thanks for helping. Syntari is a privacy-first desktop app (Tauri v2, React, Rust) for macOS and Windows.

## Setup

Follow **Development & Setup** in the [README](README.md#7-development--setup) for prerequisites on each OS. The short version:

```bash
npm install
export FOUNDRY_LOCAL_RUNTIME_VERSION=2.1.0   # PowerShell: $env:FOUNDRY_LOCAL_RUNTIME_VERSION="2.1.0"
npm run tauri dev
```

The first run downloads about 4 GB of models.

## Before opening a PR

CI runs these on both macOS and Windows; run them locally first:

```bash
npm run tsc
npm test
npm run build
cd src-tauri && cargo check && cargo test
```

## Guidelines

- **Privacy is the product.** Do not add network calls, telemetry, analytics or crash reporting. Do not loosen the CSP or Tauri capabilities without explaining why in the PR.
- **Both platforms.** Audio code is platform-specific (`src-tauri/src/audio/`). Say which OS you tested on; if you could only test one, say so and keep the other compiling.
- **Database changes** need a new numbered file in `src-tauri/migrations/` and registration in `lib.rs`. Never edit an existing migration.
- **Keep it small.** One change per PR, with a test where there is logic (Vitest for `src/`, `cargo test` for `src-tauri/`).
- **Docs.** If behaviour a user can see changes, update the README (and its project tree if you add files).
- Commit messages follow the existing style: `feat:`, `fix:`, `docs:` and so on.

## Reporting bugs

Use the issue templates. Include OS and version, app version (header), and the relevant lines from `syntari.log` (the *Logs* button opens its folder). Skim the log first, since it can contain note and transcript text. Security issues go through [SECURITY.md](SECURITY.md), not public issues.
