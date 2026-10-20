# Syntari: Air-Gapped Meeting Synthesizer

Syntari is a privacy-first, 100% offline desktop meeting synthesizer and contextual shorthand expander designed for software architects, engineering leads, and technical executives. Built on Tauri v2, React, and Foundry Local ONNX models, Syntari converts brief shorthand keystrokes into grounded, structured meeting records in real time—with zero telemetry and zero network egress.

---

## 1. Why Syntari? The Idea & Problem Space

### The Cloud Note-Taker Dilemma

Modern AI note-takers (e.g., Otter.ai, Fireflies, Grain) depend on third-party cloud infrastructure. They send internal audio streams to remote servers, create third-party vendor risks, and often rely on intrusive bot participants entering Zoom, Teams, or Google Meet calls. In air-gapped, defense, banking, or enterprise engineering environments, sending unencrypted conversation audio over the wire violates strict compliance and IP policies.

### The Problem of Passive Summarization

Most generative AI summary tools output generic, unstructured walls of text. They fail to understand what matters to an Architect:

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

* **Strict Air-Gap & Zero Egress:** Enforced via OS-level sandboxing, a zero-egress Content Security Policy (CSP), and local model inference. No audio, transcript chunks, or user keystrokes leave the local host machine.
* **Volatile RAM Audio Only:** Raw microphone and loopback audio reside exclusively in temporary circular buffers. No unencrypted `.wav` or `.pcm` files are written to disk.
* **Zero External Drivers:** Captures macOS system audio natively via ScreenCaptureKit and Windows audio via WASAPI loopback, eliminating the need for virtual audio cables (e.g., BlackHole).
* **Local Hardware Acceleration:** Runs Whisper for speech-to-text and Qwen 2.5 for text expansion using Apple Silicon Metal / CoreML on macOS and DirectML / NPU execution on Windows.
* **Encrypted At-Rest Storage:** Session records, structured notes, and transcripts are encrypted using SQLCipher, with keys stored in the OS credential vault (macOS Keychain / Windows Credential Manager).

---

## 3. System Architecture

```text
┌────────────────────────────────────────────────────────────────────────┐
│                               Frontend                                 │
│  React 18 + TypeScript + Tailwind CSS + Zustand (Strict Air-Gap CSP)   │
│                                                                        │
│  ┌───────────────────────┐  ┌────────────────────┐  ┌───────────────┐  │
│  │ Shorthand Scratchpad  │  │  Architect View    │  │  Transcript   │  │
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
│  │ • Mic (cpal)                      │  │ • whisper-tiny (ONNX)     │  │
│  │ • System Loopback (ScreenCapture) │  │   In-memory STT streaming │  │
│  │ • 16 kHz Downmix & Resampler      │  │ • qwen2.5-0.5b (WebGPU)   │  │
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
| **Network Egress** | CSP restricts connections to `default-src 'self'`; network sockets are disabled in frontend configuration. |
| **Audio Forensics** | Slices are processed in RAM circular buffers. No temporary audio files are written to `/tmp` or permanent storage. |
| **Disk Inspection** | Local databases use SQLCipher AES-256 encryption. The encryption key is randomly generated and anchored to the OS Keychain / Credential Manager. |
| **Inference Leaks** | Models run in-process through the Foundry Local ONNX runtime without external API calls or remote checkpoints. |
| **Feedback Leaks** | ScreenCaptureKit excludes Syntari's process ID to prevent application self-capture and feedback loops. |

---

## 6. Project Structure

```text
syntari/
├── src/                               # Frontend (React + TypeScript)
│   ├── components/
│   │   ├── Header.tsx                 # Session timer, air-gap badge, visualizers
│   │   ├── ShorthandPad.tsx           # Scratchpad input, dynamic timestamp sync
│   │   ├── ArchitectDashboard.tsx     # Structured cards (Decisions, Actions, Risks)
│   │   └── TranscriptDrawer.tsx       # Live dual-track transcript viewer
│   ├── store.ts                       # Zustand state management & IPC listeners
│   └── types.ts                       # Domain types & shorthand parser
├── src-tauri/                         # Backend (Rust)
│   ├── src/
│   │   ├── ai/
│   │   │   ├── engine.rs              # Foundry Local SDK bindings (Whisper + Qwen)
│   │   │   └── mod.rs                 # Lookahead time window math & prompts
│   │   ├── audio/
│   │   │   ├── mod.rs                 # Audio pipeline coordinator & ring buffers
│   │   │   ├── dsp.rs                 # Resampling, NLMS filter, frame ducking
│   │   │   └── macos_sc_tap.rs        # Native ScreenCaptureKit loopback tap
│   │   ├── lib.rs                     # Tauri IPC command handlers
│   │   └── main.rs                    # Application entry point
│   ├── resources/
│   │   └── models/                    # Offline model weights
│   │       ├── whisper-tiny/          # ONNX audio encoder/decoder & configs
│   │       └── qwen2.5-0.5b/          # ONNX text model & tokenizer configs
│   └── Cargo.toml                     # Rust dependencies & platform targets
└── tauri.conf.json                    # Tauri v2 bundle, CSP, and window config
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

### Model Staging

Download the required ONNX model weights directly to the resource path:

```bash
foundry model download openai-whisper-tiny --output src-tauri/resources/models/whisper-tiny
foundry model download qwen2.5-0.5b --output src-tauri/resources/models/qwen2.5-0.5b
```

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
