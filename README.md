# Syntari: Private On-Device Meeting Synthesizer

Syntari is a privacy-first, on-device desktop meeting synthesizer and contextual shorthand expander designed for software architects, engineering leads, and technical executives. Built on Tauri v2, React, and Foundry Local ONNX models, Syntari converts brief shorthand keystrokes into grounded, structured meeting records in real time and, on demand, a full summary of the transcript—with no telemetry of its own. After a one-time model download it runs fully offline; no audio, transcripts or notes ever leave your device.


### About the name

**Syntari** is a blend:

* **Synth-**: synthesis. The app takes messy inputs (live dual-track transcripts and brief human shorthand) and synthesizes them in real time into structured artifacts: Decisions, Actions, Risks.
* **-tari**: a nod to **Tauri**, the Rust desktop framework behind the cross-platform runtime, strict CSP and sandboxing.
* **Notary / syntax**: phonetically it suggests a digital notary (a neutral record-keeper for decisions) and the extraction of structured syntax from raw speech.

---

## 1. Why Syntari? The Idea & Problem Space

### The Cloud Note-Taker Dilemma

Modern AI note-takers (e.g., Otter.ai, Fireflies, Grain) depend on third-party cloud infrastructure. They send internal audio streams to remote servers, create third-party vendor risks, and often rely on intrusive bot participants entering Zoom, Teams, or Google Meet calls. In air-gapped, defense, banking, or enterprise engineering environments, sending unencrypted conversation audio over the wire violates strict compliance and IP policies.

### The Problem of Passive Summarization

Most generative AI summary tools output generic, unstructured walls of text. They fail to understand what matters to an architect:

* **Passive summaries** blur specific architectural decisions, trade-offs, and deadlines into generic prose.
* **Pure transcripts** require manual post-meeting review to extract actionable items.
* **Fully automated summaries** frequently hallucinate facts or miss the context of brief spoken remarks.

### The Syntari Solution: Active Shorthand + Temporal AI Grounding

Syntari pairs human intentionality with local AI:

1. **The Human Directs Intent:** The user types quick notes during the meeting (e.g., `pricing set to $45 from November`). The local model files each note as a decision, action, risk, or discussion point. Optional shorthand tags (`dec:`, `act:`, `risk:`) skip the model and choose the category directly.
2. **The System Captures Context:** Syntari records audio continuously in volatile RAM buffers and transcribes it live.
3. **The Local AI Expands Factually:** Using a focused lookback slice ($[T - 45\text{s}, T + 10\text{s}]$ relative to keystroke time), an on-device language model enriches the shorthand note with the exact dialogue spoken in that window.
4. **Zero-Hallucination Fallback:** If the shorthand topic was not explicitly discussed in the audio slice, the model is prompted to keep the user's raw note rather than fabricate details (a prompt rule, not a guarantee; small models can still err).
5. **Summary on Demand:** After you stop, *Generate summary* produces an overview, key points, decisions and action items from the whole transcript. It is never generated automatically.

---

## 2. Core Pillars & Design Constraints

* **No Data Egress:** Enforced via OS-level sandboxing, a zero-egress Content Security Policy (CSP), and local model inference. The app never transmits audio, transcripts, notes or keystrokes. Caveat: the closed-source Foundry Local native runtime may make its own metadata/telemetry requests (Microsoft's CLI build logs a telemetry init); run `scripts/check-egress.sh` to see exactly which connections your build makes.
* **Volatile RAM Audio Only:** Raw audio lives in RAM circular buffers. The one exception: each chunk sent to Whisper is briefly staged as a plaintext, owner-only (0600) `.wav` in the OS temp dir (the Foundry SDK requires a file URI), then zero-overwritten and deleted. A crash mid-transcription can leave one chunk behind; full-disk-encryption (FileVault/BitLocker) is recommended.
* **Zero External Drivers:** Captures macOS system audio natively via ScreenCaptureKit and Windows audio via WASAPI loopback, eliminating the need for virtual audio cables (e.g., BlackHole).
* **Local Hardware Acceleration:** Runs Whisper for speech-to-text and Phi-4-mini for text expansion through Foundry Local's execution providers (WebGPU on Apple Silicon for the chat model, CPU for speech). Windows DirectML/NPU support comes from Foundry but is untested here.
* **On-Device Storage:** Session records, structured notes, transcripts and summaries are stored in a plain SQLite database in the app-data directory. Nothing leaves the machine; full-disk encryption (FileVault/BitLocker) is recommended for at-rest protection.

---

## 3. System Architecture

```text
┌────────────────────────────────────────────────────────────────────────────┐
│           FRONTEND: React 18 + TypeScript + Tailwind 3 + Zustand           │
│                                                                            │
│                    Zero-egress CSP (default-src 'self')                    │
│                                                                            │
│   ┌────────────────────┐  ┌────────────────────┐  ┌────────────────────┐   │
│   │   Shorthand Pad    │  │ Notes/Summary/MD   │  │ Transcript Drawer  │   │
│   │ (raw tagged notes) │  │   (schema cards)   │  │    (audio log)     │   │
│   └────────────────────┘  └────────────────────┘  └────────────────────┘   │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
               │                       ▲                       ▲
               │ invoke                │ event                 │ event
               │ expand_note,          │ note-expanded         │ transcript-chunk
                │ summarize_transcript  │
               ▼                       │                       │
┌────────────────────────────────────────────────────────────────────────────┐
│                            TAURI v2 CORE (Rust)                            │
│                                                                            │
│ ┌──────────────────────────────────┐  ┌──────────────────────────────────┐ │
│ │           AUDIO ENGINE           │  │        FOUNDRY AI ENGINE         │ │
│ │──────────────────────────────────│  │──────────────────────────────────│ │
│ │• Mic: cpal (both OS)             │  │• whisper-small (ONNX)            │ │
│ │• System audio:                   │  │    chunked speech-to-text        │ │
│ │    macOS: ScreenCaptureKit       │  │• phi-4-mini                      │ │
│ │    Windows: WASAPI loopback      │  │    classify / expand / insights  │ │
│ │• 16 kHz downmix + resampler      │  │• Anti-hallucination prompts      │ │
│ │• NLMS echo canceller             │  │• Models: first-run download,     │ │
│ │• Frame-sliced ducking (-18 dB)   │  │    SHA-256 verified              │ │
│ │• In-memory RAM ring buffer       │  │• MIT runtime bundled in app      │ │
│ └──────────────────────────────────┘  └──────────────────────────────────┘ │
│                                                                            │
│       ┌────────────────────────────────────────────────────────────┐       │
│       │                  SQLite, on-device only                    │       │
│       └────────────────────────────────────────────────────────────┘       │
│                                                                            │
└────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Note Classification & Shorthand

Type plain notes; no syntax is required. In a single model call, phi-4-mini classifies each note (`decision`, `action`, `risk`, or `discussion`) and expands it from the surrounding transcript. For actions it also extracts the owner and due date when stated. A note appears immediately as Discussion and moves to its card when the model answers; if the answer is missing or unrecognised it stays in Discussion. Notes you type are listed in the left pad; AI suggestions you accept appear only in the middle panel. Every note has a hover *Move to* menu to correct the category, and a manual choice is never overwritten.

Optional shorthand prefixes skip classification and set the category instantly (case-insensitive):

| Prefix | Schema Category | Example Input | Expanded Output |
| :---

## 5. Security & Threat Model

| Vector | Mitigation |
| :--- | :--- |
| **Network Egress** | CSP restricts the frontend to `default-src 'self'`. Backend network access is the one-time model download on first run (see Models), plus whatever the Foundry Local runtime does internally (unverified; see `scripts/check-egress.sh`). Syntari itself never transmits user data. |
| **Audio Forensics** | Audio is held in RAM ring buffers. Each slice sent to Whisper is staged for the duration of transcription as an owner-only (0600) `.wav` in the OS temp dir (the Foundry SDK needs a file path), then zero-overwritten and deleted. A crash mid-transcription can leave one slice behind; use FileVault/BitLocker. |
| **Disk Inspection** | The local database is plain SQLite in the app-data directory (no at-rest encryption; use FileVault/BitLocker). Model files are SHA-256 recorded after first download and verified on every launch (trust-on-first-use). |
| **Inference Leaks** | Models run in-process through the Foundry Local ONNX runtime; Syntari makes no inference API calls. Weights are fetched once, then used from the local cache. |
| **Feedback Leaks** | Mic audio is ducked and echo-cancelled against system audio, and the mic track is skipped while system audio is playing, to limit speaker bleed. |

---

## 6. Project Structure

```text
syntari/
├── src/                               # Frontend (React + TypeScript)
│   ├── components/
│   │   ├── Header.tsx                 # Session timer, privacy badge, visualizers
│   │   ├── ShorthandPad.tsx           # Scratchpad input, dynamic timestamp sync
│   │   ├── NotesDashboard.tsx         # Notes / Summary / Markdown tabs
│   │   ├── Meetings.tsx               # Meetings home: day-grouped history, search, Open / Continue
│   │   └── TranscriptDrawer.tsx       # Live dual-track transcript viewer
│   ├── store.ts                       # Zustand state, SQLite persistence, session history
│   └── types.ts                       # Domain types & shorthand parser
├── src-tauri/                         # Backend (Rust)
│   ├── src/
│   │   ├── ai/
│   │   │   ├── engine.rs              # Foundry Local SDK: model download/verify/load, STT, expansion, insights, summary
│   │   │   └── mod.rs                 # Lookback/lookahead window math (+ tests)
│   │   ├── audio/
│   │   │   ├── mod.rs                 # Audio pipeline coordinator & ring buffers
│   │   │   ├── dsp.rs                 # Resampling, NLMS filter, frame ducking
│   │   │   ├── macos_sc_tap.rs        # Native ScreenCaptureKit loopback tap (macOS)
│   │   │   ├── loopback_macos.rs      # Virtual-device fallback (macOS)
│   │   │   └── loopback_windows.rs    # WASAPI loopback via cpal (Windows)
│   │   ├── lib.rs                     # Tauri IPC command handlers, SQL plugin + migrations
│   │   └── main.rs                    # Application entry point
│   ├── migrations/                    # 001_init.sql (sessions, notes, transcript); 002_summary.sql (summary, auto flag)
│   ├── tauri.conf.json                # Tauri v2 bundle, CSP, and window config
│   ├── tauri.windows-bundle.conf.json # Extra resources for the Windows installer (runtime DLLs, notices)
│   ├── licenses/                      # MIT license + third-party notices for the bundled Foundry runtime
│   ├── icons/                         # App icons (placeholder; regenerate with `tauri icon`)
│   └── Cargo.toml                     # Rust dependencies & platform targets
├── .github/workflows/windows.yml      # Windows CI: tsc, vitest, frontend build, cargo check; manual installer build
└── scripts/check-egress.sh            # Lists the running app's network sockets

Models are NOT in the repo or the bundle; they live in the app data dir (see Models).
```

---

## 7. Development & Setup

### Prerequisites

* macOS 14+ (Apple Silicon recommended) or Windows 11 (x64 or ARM64)
* Node.js 18+ and Rust 1.75+
* **macOS:** Foundry CLI (provides the native runtime Syntari loads):
  ```bash
  brew tap microsoft/foundrylocal
  brew install foundrylocal
  ```
* **Windows:** no Foundry install needed; the SDK downloads its native runtime (including WinML) at build time, and the installer bundles it (`src-tauri/tauri.windows-bundle.conf.json`, staged by CI; for a local installer copy the `*.dll` files next to `foundry_local.dll` in `src-tauri/target/**/out` into `src-tauri/native/` and run `npm run tauri build -- --config src-tauri/tauri.windows-bundle.conf.json`). Instead install: Visual Studio Build Tools (C++ workload), [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on Windows 11), [Strawberry Perl](https://strawberryperl.com/) and [NASM](https://www.nasm.us/) on `PATH` (still required while the SQLCipher/vendored-OpenSSL build flag remains in `Cargo.toml`, although the database is not encrypted), and Rust's `stable-x86_64-pc-windows-msvc` toolchain.

### Models

Models are not bundled or committed. On first launch Syntari downloads whisper-small and phi-4-mini once into the app data dir (`~/Library/Application Support/com.syntari.app/models` on macOS, `%APPDATA%\\com.syntari.app\\models` on Windows). Defaults: whisper-small (~440 MB, speech) and phi-4-mini (~3.7 GB, expansion/insights); expect a ~4 GB first download and ~4 GB RAM while running. This is the only network access Syntari itself performs.

* **Air-gapped / manual install:** set `SYNTARI_MODELS_DIR` to a folder pre-populated with models (e.g. `foundry model download ... --output <dir>` on a connected machine, then copy it over).
* **Updating models:** delete the model folder and relaunch (re-downloads), or replace the files in `SYNTARI_MODELS_DIR`.
* **Dev with your existing models:** `SYNTARI_MODELS_DIR=$PWD/src-tauri/resources/models npm run tauri dev`

### Using the App

* **Meetings:** the app opens on a Meetings list grouped by day, with search across summaries, notes and transcripts. *New meeting* opens a blank session; *Open* views a past meeting; *Continue* resumes it in place (new audio, notes and transcript are appended and the timer carries on).
* **Start / Stop** a session from the header (*Continue* on a reopened meeting); the left pad takes your notes, the middle panel structures them, the right drawer shows the live transcript. *← Meetings* returns to the list when no session is running.
* **AI suggestions:** every 30 s the local model proposes decisions/actions/risks from the transcript. They appear only in the Notes panel (amber) until you **Accept** or **Dismiss** them.
* **Summary:** after stopping, open the *Summary* tab and click *Generate summary* (or *Regenerate*). Long transcripts are summarised in segments and merged. It is saved with the meeting.
* **Export:** *Copy Markdown* copies the summary (if generated) and your accepted notes to the clipboard; *Export .md* saves the same content to `Downloads` in your home folder (`%USERPROFILE%` on Windows) and reveals the file in Finder / Explorer.
* **Moving notes:** hover a note and use its dropdown to move it to another card.
* **Status pill:** shows one-time model download progress and model errors. *Start* stays disabled until the models are loaded.

### Running Locally

```bash
# Install frontend dependencies
npm install

# Run checks
npm run tsc
cd src-tauri && cargo check && cd ..

# Launch desktop app in development
npm run tauri dev
```

---

## 8. Known Limitations & Troubleshooting

* **First launch is slow:** ~4 GB of models download once (progress in the header). Transcription is unavailable until it finishes.
* **Transcript accuracy:** speech is cut on silence gaps (3 s minimum, 12 s maximum) and transcribed by whisper-small. Larger models (`whisper-medium`, `whisper-large-v3-turbo`) are in the Foundry catalog but not wired up. Non-speech tags like `(soft music)` are discarded.
* **You-track during remote audio:** the mic track is kept while system audio plays only if its energy after echo cancellation exceeds 0.005 RMS (`audio/mod.rs`). The threshold is untuned; speaker bleed may be transcribed as "You" or quiet speech dropped. Use headphones for best results.
* **Exit behaviour:** Foundry's native runtime aborts in a C++ destructor on normal process exit (macOS "quit unexpectedly" dialog). Syntari stops the session and then exits with `_exit(0)` (macOS/Linux) or `TerminateProcess` (Windows) to skip it.
* **Foundry runtime is closed source:** it may make its own telemetry/metadata requests; verify with `scripts/check-egress.sh` or a network monitor.
* **Foundry licensing (reviewed, not legal advice):** the `foundry-local-sdk` crate and the `Microsoft.AI.Foundry.Local.Runtime` NuGet package (the native `foundry_local` library plus ONNX Runtime / GenAI, which the build downloads for Windows and macOS) are **MIT**, so bundling them in an installer is permitted if you ship the MIT notice and `ThirdPartyNotices` from the package. The **Foundry Local CLI** installed via `brew`/`winget` is under separate Microsoft terms that forbid combining it with your application for others to use, so do not redistribute the CLI or its `libfoundry_local.dylib`. Model weights are fetched from the Foundry catalog at first run, not bundled; whisper-small and phi-4-mini are MIT-licensed upstream, but confirm the catalog entries' terms before bundling weights. The macOS build currently loads the Homebrew copy (see `ai/engine.rs`); for distribution, drop that lookup and use the SDK's own downloaded runtime.
* **Classification accuracy:** phi-4-mini is a small model; ambiguous or tentative notes may be mis-filed. Use the *Move to* menu. Export has no save dialog (always `~/Downloads`).
* **Windows is new and untested on hardware:** it compiles in CI but needs a real run. Differences from macOS: system audio uses WASAPI loopback on the default output device (no permission prompt; capture follows the device's mix format, usually 48 kHz stereo); only the microphone privacy toggle matters (*Settings > Privacy > Microphone*); on exit the process is ended with `TerminateProcess` (the Windows counterpart of `_exit`) to skip Foundry's teardown, so confirm there is no crash dialog on quit; the icons in `src-tauri/icons/` are a generic placeholder; replace them with `npm run tauri icon <1024px.png>`.
* **Data is unencrypted at rest:** the database is plain SQLite in the app-data directory (`syntari.db`). Encrypting it would need our own SQLCipher connection, because the SQL plugin cannot pass a key. Use FileVault/BitLocker.
* **Summary quality and speed:** the summary comes from phi-4-mini on-device, so it can miss points or be slow on long meetings. There is no title or edit step; the Meetings list labels a meeting with the start of its summary. Regenerate after continuing a meeting, since the old summary is not updated automatically.
* **Continue and AI context:** continuing restores notes and transcript for display and summary, but the in-memory window used for note expansion only covers audio recorded since you resumed.
* **Old accepted suggestions:** notes accepted from AI suggestions before migration 002 have no AI flag, so they show in the left pad when reloaded.
* **No delete:** meetings cannot be deleted from the UI.
* **Not implemented:** signed/notarized release builds, upstream-pinned model hashes.


---

## 9. Platform Support & Change Log

| | macOS | Windows |
| :--- | :--- | :--- |
| Mic capture | cpal | cpal |
| System audio | ScreenCaptureKit (permission prompt) | WASAPI loopback, default output device (no prompt) |
| Foundry runtime | Homebrew copy today (bundling is a follow-up) | Downloaded by the SDK at build time; bundled in the installer |
| Exit workaround | `_exit(0)` | `TerminateProcess` |
| Status | Primary, tested | Compiles in CI; awaiting hardware test |

**Recent changes**

* **Meetings, summary and fixes:** Meetings home screen with search and Continue; on-demand full summary; transcript sorted by time with sticky-bottom scrolling; left pad shows only typed notes; the database now opens as plain SQLite (the earlier keyed URL was rejected, so nothing was being saved) and the keychain code was removed.
* **Windows support:** export path falls back to `USERPROFILE` and reveals the file in Explorer; settings shortcut opens the Windows microphone privacy page; Foundry library name is platform-aware; Windows exit handler; real (placeholder) icons so the bundle builds.
* **CI:** `.github/workflows/windows.yml` runs `tsc`, `vitest`, the frontend build and `cargo check` on `windows-latest`. A manual run also stages the runtime DLLs, builds the NSIS installer, verifies it contains `foundry_local.dll`, and uploads it as an artifact.
* **Installer bundling:** the MIT Foundry runtime DLLs and license notices ship next to the exe (see licensing in section 8); a failed runtime load shows a clear "reinstall" message.
* **Dependencies:** Tailwind pinned to 3.x (the v4 bump broke the frontend build); `@vitejs/plugin-react` 6 for Vite 8.
* **Earlier:** AI note classification, UI refresh, mic/AEC alignment fix, export to `.md`, model-extracted owner/due for actions, session history, first-run model download.

---

## 10. Testing on Windows

CI (`.github/workflows/windows.yml`) runs `tsc`, `vitest` and `cargo check` on `windows-latest` for every push and PR. Running it manually (*Actions > windows > Run workflow*) also builds the installer and attaches it as the `syntari-windows` artifact.

**Tester steps (from source):**

1. Install the Windows prerequisites in section 7, then `git clone` the repo and `git checkout main`.
2. `npm install`, then `npm run tauri dev`. The first Rust build is long (Foundry runtime download plus vendored OpenSSL).
3. Wait for the ~4 GB model download (header status pill) before starting a session.
4. Run this checklist and report pass/fail with the console output:
   * Mic only: *Start*, speak, transcript appears as "You".
   * System audio: play a YouTube video through speakers/headphones; remote speech appears in the transcript. Try a different default output device and Bluetooth headphones.
   * Notes: type shorthand (e.g. `act:`), check classification and AI suggestions.
   * Stop, click *Generate summary*, go to *← Meetings*, open the session, and restart the app to confirm it persists; try *Continue* on it.
   * *Export .md* writes to `%USERPROFILE%\Downloads` and Explorer opens with the file selected.
   * Quit the app: note whether a crash dialog appears.
   * With a mic privacy toggle off, the *open settings* hint opens the Windows Microphone privacy page.
5. Optional: capture network connections during use (Resource Monitor or Wireshark) to confirm the only traffic is the model download.

**Tester steps (from CI artifact):** download `syntari-windows` from the workflow run and install the `.exe`. Unsigned, so SmartScreen will warn (*More info > Run anyway*).
