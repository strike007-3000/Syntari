# Syntari: Private On-Device Meeting Synthesizer

Syntari is a privacy-first, on-device desktop meeting synthesizer and contextual shorthand expander designed for software architects, engineering leads, and technical executives. Built on Tauri v2, React, and Foundry Local ONNX models, Syntari converts brief shorthand keystrokes into grounded, structured meeting records in real time—with no telemetry of its own. After a one-time model download it runs fully offline; no audio, transcripts or notes ever leave your device.

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

1. **The Human Directs Intent:** The user types quick shorthand tags during the meeting (e.g., `dec: pricing $45 Nov`, `act: Sarah Deploy fix Fri`, `risk: SOC2 latency`).
2. **The System Captures Context:** Syntari records audio continuously in volatile RAM buffers.
3. **The Local AI Expands Factually:** Using a focused lookback slice ($[T - 45\text{s}, T + 10\text{s}]$ relative to keystroke time), an on-device language model enriches the shorthand note with the exact dialogue spoken in that window.
4. **Zero-Hallucination Fallback:** If the shorthand topic was not explicitly discussed in the audio slice, the model retains the user's raw note rather than fabricating details.

---

## 2. Core Pillars & Design Constraints

* **No Data Egress:** Enforced via OS-level sandboxing, a zero-egress Content Security Policy (CSP), and local model inference. The app never transmits audio, transcripts, notes or keystrokes. Caveat: the closed-source Foundry Local native runtime may make its own metadata/telemetry requests (Microsoft's CLI build logs a telemetry init); run `scripts/check-egress.sh` to see exactly which connections your build makes.
* **Volatile RAM Audio Only:** Raw audio lives in RAM circular buffers. The one exception: each chunk sent to Whisper is briefly staged as a plaintext, owner-only (0600) `.wav` in the OS temp dir (the Foundry SDK requires a file URI), then zero-overwritten and deleted. A crash mid-transcription can leave one chunk behind; full-disk-encryption (FileVault/BitLocker) is recommended.
* **Zero External Drivers:** Captures macOS system audio natively via ScreenCaptureKit and Windows audio via WASAPI loopback, eliminating the need for virtual audio cables (e.g., BlackHole).
* **Local Hardware Acceleration:** Runs Whisper for speech-to-text and Phi-4-mini for text expansion through Foundry Local's execution providers (WebGPU on Apple Silicon for the chat model, CPU for speech). Windows DirectML/NPU support comes from Foundry but is untested here.
* **Encrypted At-Rest Storage:** Session records, structured notes, and transcripts are encrypted using SQLCipher, with a random 256-bit key stored in the OS credential vault (macOS Keychain / Windows Credential Manager). If the keychain is unavailable, nothing is persisted (there is no unencrypted fallback).

---

## 3. System Architecture

```text
┌────────────────────────────────────────────────────────────────────────┐
│                               Frontend                                 │
│  React 18 + TypeScript + Tailwind CSS + Zustand (Zero-Egress CSP)   │
│                                                                        │
│  ┌───────────────────────┐  ┌────────────────────┐  ┌───────────────┐  │
│  │ Shorthand Scratchpad  │  │  Notes View        │  │  Transcript   │  │
│  │ (Raw Tagged Notes)    │  │  (Schema Cards)    │  │  (Audio Log)  │  │
│  └───────────┬───────────┘  └─────────▲──────────┘  └───────▲───────┘  │
└──────────────┼────────────────────────┼─────────────────────┼──────────┘
               │ invoke("expand_note")   │ "note-expanded"     │ "stt-chunk"
               ▼                        │                     │
┌────────────────────────────────────────────────────────────────────────┐
│                        Tauri v2 Core (Rust)                            │
│                                                                        │
│  ┌───────────────────────────────────┐  ┌───────────────────────────┐  │
│  │           Audio Engine            │  │     Foundry AI Engine     │  │
│  │ ───────────────────────────────── │  │ ───────────────────────── │  │
│  │ • Mic (cpal)                      │  │ • whisper-small (ONNX)     │  │
│  │ • System Loopback (ScreenCapture) │  │   In-memory STT streaming │  │
│  │ • 16 kHz Downmix & Resampler      │  │ • phi-4-mini (WebGPU)   │  │
│  │ • NLMS Echo Canceller (2048 taps) │  │   Contextual Expansion    │  │
│  │ • Frame-Sliced Ducking (-18 dB)   │  │ • Anti-hallucination      │  │
│  │ • Circular In-Memory RAM Buffer   │  │   prompt templates        │  │
│  └───────────────────────────────────┘  └───────────────────────────┘  │
│                                                                        │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │ Local Persistence: SQLCipher (AES-256) via OS Credential Vault   │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Shorthand Syntax Reference

Syntari automatically routes and structures notes when prefixed with shorthand tokens:

| Prefix | Schema Category | Example Input | Expanded Output |
| :--- | :--- | :--- | :--- |
| `dec:` | Decisions | `dec: pricing $45 Nov` | Enterprise pricing tier set to $45 starting November. |
| `act:` | Action Items | `act: Sarah API audit Fri` | Sarah to complete the API security audit before Friday's release. |
| `risk:` | Risks & Blockers | `risk: DB lockup on migrate` | Risk identified regarding potential database lockup during table migration. |
| *(none)* | Discussion Points | `CGI rendering pipeline` | Architectural discussion regarding moving the CGI rendering pipeline to WebGPU. |

---

## 5. Security & Threat Model

| Vector | Mitigation |
| :--- | :--- |
| **Network Egress** | CSP restricts the frontend to `default-src 'self'`. Backend network access is the one-time model download on first run (see Models), plus whatever the Foundry Local runtime does internally (unverified; see `scripts/check-egress.sh`). Syntari itself never transmits user data. |
| **Audio Forensics** | Audio is held in RAM ring buffers. Each slice sent to Whisper is staged for the duration of transcription as an owner-only (0600) `.wav` in the OS temp dir (the Foundry SDK needs a file path), then zero-overwritten and deleted. A crash mid-transcription can leave one slice behind; use FileVault/BitLocker. |
| **Disk Inspection** | Local databases use SQLCipher AES-256 encryption with a random key anchored in the OS Keychain / Credential Manager. Without a keychain key the app saves nothing. Model files are SHA-256 recorded after first download and verified on every launch (trust-on-first-use). |
| **Inference Leaks** | Models run in-process through the Foundry Local ONNX runtime in-process; Syntari makes no inference API calls. Weights are fetched once, then used from the local cache. |
| **Feedback Leaks** | Mic audio is ducked and echo-cancelled against system audio, and the mic track is skipped while system audio is playing, to limit speaker bleed. |

---

## 6. Project Structure

```text
syntari/
├── src/                               # Frontend (React + TypeScript)
│   ├── components/
│   │   ├── Header.tsx                 # Session timer, privacy badge, visualizers
│   │   ├── ShorthandPad.tsx           # Scratchpad input, dynamic timestamp sync
│   │   ├── NotesDashboard.tsx         # Structured cards (Decisions, Actions, Risks)
│   │   └── TranscriptDrawer.tsx       # Live dual-track transcript viewer
│   ├── store.ts                       # Zustand state, SQLCipher persistence, session history
│   └── types.ts                       # Domain types & shorthand parser
├── src-tauri/                         # Backend (Rust)
│   ├── src/
│   │   ├── ai/
│   │   │   ├── engine.rs              # Foundry Local SDK: model download/verify/load, STT, expansion, insights
│   │   │   └── mod.rs                 # Lookback/lookahead window math (+ tests)
│   │   ├── audio/
│   │   │   ├── mod.rs                 # Audio pipeline coordinator & ring buffers
│   │   │   ├── dsp.rs                 # Resampling, NLMS filter, frame ducking
│   │   │   └── macos_sc_tap.rs        # Native ScreenCaptureKit loopback tap
│   │   ├── lib.rs                     # Tauri IPC command handlers
│   │   └── main.rs                    # Application entry point
│   ├── migrations/001_init.sql        # Sessions, notes, transcript schema
│   ├── tauri.conf.json                # Tauri v2 bundle, CSP, and window config
│   └── Cargo.toml                     # Rust dependencies & platform targets
└── scripts/check-egress.sh            # Lists the running app's network sockets

Models are NOT in the repo or the bundle; they live in the app data dir (see Models).
```

---

## 7. Development & Setup

### Prerequisites

* macOS 14+ (Apple Silicon recommended) or Windows 11
* Node.js 18+ and Rust 1.75+
* Foundry CLI:
  ```bash
  brew tap microsoft/foundrylocal
  brew install foundrylocal
  ```

### Models

Models are not bundled or committed. On first launch Syntari downloads whisper-small and phi-4-mini once into the app data dir (`~/Library/Application Support/com.syntari.app/models` on macOS). Defaults: whisper-small (~440 MB, speech) and phi-4-mini (~3.7 GB, expansion/insights); expect a ~4 GB first download and ~4 GB RAM while running. This is the only network access Syntari itself performs.

* **Air-gapped / manual install:** set `SYNTARI_MODELS_DIR` to a folder pre-populated with models (e.g. `foundry model download ... --output <dir>` on a connected machine, then copy it over).
* **Updating models:** delete the model folder and relaunch (re-downloads), or replace the files in `SYNTARI_MODELS_DIR`.
* **Dev with your existing models:** `SYNTARI_MODELS_DIR=$PWD/src-tauri/resources/models npm run tauri dev`

### Using the App

* **Start / Stop** a session from the header; the left pad takes your shorthand, the middle panel structures it, the right drawer shows the live transcript.
* **AI suggestions:** every 30 s the local model proposes decisions/actions/risks from the transcript. They appear only in the Notes panel (amber) until you **Accept** or **Dismiss** them.
* **History:** when no session is running, the header's *History* dropdown reloads any past session (notes + transcript).
* **Export:** *Copy Markdown* copies the current notes to the clipboard.
* **Status pill:** shows one-time model download progress, model errors, or "Keychain unavailable: notes will not be saved".

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

* **Keychain prompt on every dev rebuild:** unsigned dev builds are a new app to macOS each time; enter your login password and choose *Always Allow*. Denying means no persistence.
* **First launch is slow:** ~4 GB of models download once (progress in the header). Transcription is unavailable until it finishes.
* **Transcript accuracy:** speech is cut on silence gaps (3 s minimum, 12 s maximum) and transcribed by whisper-small. Larger models (`whisper-medium`, `whisper-large-v3-turbo`) are in the Foundry catalog but not wired up. Non-speech tags like `(soft music)` are discarded.
* **You-track gaps:** the mic track is skipped while system audio is playing (echo canceller not converged), so speech during remote audio is not transcribed.
* **Exit behaviour:** Foundry's native runtime aborts in a C++ destructor on normal process exit (macOS "quit unexpectedly" dialog). Syntari stops the session and then exits with `_exit(0)` to skip it.
* **Foundry runtime is closed source:** it may make its own telemetry/metadata requests; verify with `scripts/check-egress.sh` or a network monitor. The native library is currently located via Homebrew paths (`brew install foundrylocal`); bundling it for distribution is not done and needs a license review.
* **Not implemented:** file export (only clipboard), signed/notarized release builds, upstream-pinned model hashes.
