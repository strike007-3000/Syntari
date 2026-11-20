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
2. **The System Captures Context:** Syntari records audio continuously in volatile RAM buffers and transcribes it live. An opt-in *Save audio* switch also writes raw audio to the app data folder (see Audio Forensics), so a crash loses almost nothing and a meeting can be transcribed again.
3. **The Local AI Expands Factually:** Using a focused lookback slice ($[T - 45\text{s}, T + 10\text{s}]$ relative to keystroke time), an on-device language model enriches the shorthand note with the exact dialogue spoken in that window.
4. **Zero-Hallucination Fallback:** If the shorthand topic was not explicitly discussed in the audio slice, the model is prompted to keep the user's raw note rather than fabricate details (a prompt rule, not a guarantee; small models can still err).
5. **Summary on Demand:** After you stop, *Generate summary* produces an overview, key points, decisions and action items from the whole transcript. It is never generated automatically.

---

## 2. Core Pillars & Design Constraints

* **No Data Egress:** Enforced via OS-level sandboxing, a zero-egress Content Security Policy (CSP), and local model inference. The app never transmits audio, transcripts, notes or keystrokes. Caveat: the closed-source Foundry Local native runtime may make its own metadata/telemetry requests (Microsoft's CLI build logs a telemetry init); run `scripts/check-egress.sh` (macOS/Unix only: it uses `pgrep` and `lsof`) to see exactly which connections your build makes.
* **Volatile RAM Audio Only:** Raw audio lives in RAM circular buffers. The one exception: each chunk sent to Whisper is briefly staged as a plaintext `.wav` in the OS temp dir (owner-only 0600 on macOS; on Windows it inherits the temp folder's permissions; the Foundry SDK requires a file URI), then zero-overwritten and deleted. A crash mid-transcription can leave one chunk behind, which is purged on the next launch; full-disk-encryption (FileVault/BitLocker) is recommended. **Opt-in exception:** the *Save audio* switch also writes the raw tracks to the app-data folder (see Security & Threat Model); it is off by default and the audio never leaves the machine.
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
               │ expand_note,          │ summarize-progress    │ transcript-chunk
               │ summarize_transcript, │                       │
               │ extract_insights      │                       │
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
│ │• Silence gap fill, pause         │  │• Stock-phrase + loop filters     │ │
│ │• Mic pick, follows device        │  │                                  │ │
│ │• Echo-line drop (audio-gated)    │  │                                  │ │
│ │• Opt-in PCM recording to disk    │  │                                  │ │
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

Type plain notes; no syntax is required. In a single model call, phi-4-mini classifies each note (`decision`, `action`, `risk`, or `discussion`) and expands it from the surrounding transcript. For actions it also extracts the owner and due date when stated. A note appears immediately as Discussion and moves to its card when the model answers; if the answer is missing or unrecognised it stays in Discussion. If the model returns the note unchanged (nothing in the transcript window matched; the pad shows *Filed*), its category is also forced to Discussion, since it would be a guess. A `dec:` / `act:` / `risk:` prefix is never overridden. Notes you type are listed in the left pad; AI suggestions you accept appear only in the middle panel. Every note has a hover *Move to* menu to correct the category, and a manual choice is never overwritten.

Optional shorthand prefixes skip classification and set the category instantly (case-insensitive):

| Prefix | Category | Example input | Parsed as |
| :--- | :--- | :--- | :--- |
| `dec:` | Decision | `dec: go with Postgres` | Decision, text `go with Postgres` |
| `act:` | Action Item | `act: Dana send contract Friday` | Action, owner `Dana`, due `Friday` (the owner is a capitalised first word followed by more text, so `act: Ship` has none; the due date is a trailing `Friday`, `tomorrow`, `eod`, `eow`, `5/12` or `2026-05-12`, optionally after `by`, `due` or `on`) |
| `risk:` | Risk | `risk: vendor delay` | Risk, text `vendor delay` |

The model still expands the text from the transcript; only the category is fixed. Notes without a prefix are classified by the model.

## 5. Security & Threat Model

| Vector | Mitigation |
| :--- | :--- |
| **Network Egress** | CSP restricts the frontend to `default-src 'self'`. Backend network access is the one-time model download on first run (see Models), plus whatever the Foundry Local runtime does internally (unverified; see `scripts/check-egress.sh`). Syntari itself never transmits user data. |
| **Audio Forensics** | Audio is held in RAM ring buffers. Each slice sent to Whisper is staged for the duration of transcription as a `.wav` in the OS temp dir (owner-only 0600 on macOS; inherited temp-folder permissions on Windows; the Foundry SDK needs a file path), then zero-overwritten and deleted. A crash mid-transcription can leave one slice behind, purged on the next launch; use FileVault/BitLocker. **Save audio** (off by default) is the exception: it writes raw 16 kHz PCM (~115 MB/hour/track, unencrypted, owner-only on macOS) to `recordings/<meeting id>` in the app data folder until you delete it or the meeting. |
| **Disk Inspection** | Release builds log to `syntari.log` in the app log dir; it can contain note and transcript-derived text (inference lines), is reset at launch once it passes 5 MB, and the *Logs* button opens the log folder. Dev builds (`npm run tauri dev`) log to the terminal instead. The local database is plain SQLite in the app-data directory (no at-rest encryption; use FileVault/BitLocker). Model files are SHA-256 recorded after first download and re-verified at launch (trust-on-first-use). A file whose size and mtime match the saved record is not re-hashed, to keep startup fast, so same-size corruption that preserves the mtime is not detected; delete the model folder to force a re-download. |
| **Inference Leaks** | Models run in-process through the Foundry Local ONNX runtime; Syntari makes no inference API calls. Weights are fetched once, then used from the local cache. |
| **Feedback Leaks** | Mic audio is ducked and echo-cancelled against system audio, and the mic track is skipped while system audio is playing unless its post-echo-cancellation energy exceeds `BLEED_RMS` (0.005), to limit speaker bleed. A mic line that repeats the remote side's recent words is also dropped, but only when the loudness of the two tracks has shown the mic following the system audio (see Known Limitations). |
| **Saved audio (opt-in)** | *Save audio* stores raw PCM under `<app-data>/recordings/<meeting id>/` (`me.pcm`, `them.pcm`; 16 kHz mono 16-bit, about 115 MB per hour per track, unencrypted). It is local only. It is removed with its meeting, by *Delete audio*, or by *Delete audio older than 30 days*. Backups (*Back up DB*) do not include it. |

---

## 6. Project Structure

```text
syntari/
├── src/                               # Frontend (React + TypeScript)
│   ├── App.tsx                        # Screen switcher (Meetings / session); event and level listeners
│   ├── components/
│   │   ├── Header.tsx                 # Timer, version, privacy badge, model/DB-error banners, meters, Pause / Stop, Export / Back up DB / Logs
│   │   ├── RecordingControls.tsx      # Save audio switch, microphone picker, silence auto-stop banner, Re-transcribe / Delete audio
│   │   ├── ShorthandPad.tsx           # Scratchpad input, dynamic timestamp sync
│   │   ├── NotesDashboard.tsx         # Notes / Summary / Markdown tabs; per-note edit (double-click) and delete
│   │   ├── AskMeetings.tsx            # Ask box on the Meetings page: answer with clickable [n] sources
│   │   ├── Meetings.tsx               # Meetings home: day-grouped history, search, Open / Rename / Continue / Delete, saved-audio sizes, Recover from audio
│   │   └── TranscriptDrawer.tsx       # Live dual-track transcript viewer
│   ├── store.ts                       # Zustand state, SQLite persistence, session history, delete, backup, DB-error state
│   ├── ask.ts / ask.test.ts           # Question to FTS5 query (stop words, quoting) and the Source type
│   ├── store.test.ts                  # Insight-poll binding and pause/resume clock tests
│   ├── types.ts                       # Domain types & shorthand parser
│   └── types.test.ts                  # Shorthand parser unit tests
├── src-tauri/                         # Backend (Rust)
│   ├── src/
│   │   ├── ai/
│   │   │   ├── engine.rs              # Foundry Local SDK: model download (disk-space check)/verify/load, STT, expansion, insights, summary
│   │   │   └── mod.rs                 # Lookback/lookahead window math (+ tests)
│   │   ├── audio/
│   │   │   ├── mod.rs                 # Audio pipeline coordinator & ring buffers; i16/u16/i32/f32 input conversion; bleed threshold
│   │   │   ├── dsp.rs                 # Resampling, NLMS filter, frame ducking, silence-gap padding, loudness-envelope correlation (+ tests)
│   │   │   ├── echo.rs                # Drops mic lines that copy the remote side, gated by audio leak evidence (+ tests)
│   │   │   ├── pause.rs               # Pause flag and paused-time clock shared by the capture callbacks and the drain loop (+ tests)
│   │   │   ├── recording.rs           # Opt-in PCM recording, listing, and re-transcription of saved audio (+ tests)
│   │   │   ├── macos_sc_tap.rs        # Native ScreenCaptureKit loopback tap (macOS)
│   │   │   ├── loopback_macos.rs      # Virtual-device fallback (macOS)
│   │   │   └── loopback_windows.rs    # WASAPI loopback via cpal (Windows)
│   │   ├── lib.rs                     # Tauri IPC commands (session, pause, recordings, microphones, export), SQL plugin + migrations, log file, startup temp-file cleanup
│   │   └── main.rs                    # Application entry point
│   ├── migrations/                    # 001_init.sql (sessions, notes, transcript); 002_summary.sql (summary, auto flag); 003_title.sql (meeting title); 004_citation_start.sql (citation timestamp); 005_transcript_excluded.sql; 006_summary_stale.sql; 007_search_index.sql (FTS5 indexes over transcript lines and notes, kept current by triggers)
│   ├── Info.plist                     # macOS usage descriptions (microphone, screen/system audio)
│   ├── capabilities/default.json      # Tauri permission set for the main window
│   ├── tauri.conf.json                # Tauri v2 bundle, CSP, and window config
│   ├── tauri.windows-bundle.conf.json # Extra resources for the Windows installer (runtime DLLs, notices)
│   │                                  # (tauri.macos-bundle.conf.json is generated by the macos workflow, not committed)
│   ├── licenses/                      # MIT license + third-party notices for the bundled Foundry runtime
│   ├── icons/                         # App icons generated from source.svg (`npm run tauri icon <1024px.png>`)
│   └── Cargo.toml                     # Rust dependencies & platform targets
├── .github/workflows/windows.yml      # Windows CI: tsc, vitest, frontend build, cargo check/test; installer build + pre-release on manual run or v* tag
├── .github/workflows/macos.yml        # macOS CI: same checks; ad-hoc-signed .app/.dmg with the Foundry dylibs, pre-release on manual run or v* tag
└── scripts/check-egress.sh            # Lists the running app's network sockets

Models are NOT in the repo or the bundle; they live in the app data dir (see Models).
```

---

## 7. Development & Setup

### Prerequisites

* macOS 13+ (the bundle minimum in `tauri.conf.json`; Apple Silicon recommended) or Windows 11 (x64 or ARM64)
* Node.js 20.19+ (or 22.12+, required by Vite 8) and a current stable Rust toolchain
* **macOS:** no Foundry install needed. Build with `FOUNDRY_LOCAL_RUNTIME_VERSION=2.3.1` (the `foundry-local-sdk` version) and the SDK downloads the MIT-licensed native runtime at build time; the `macos` workflow stages the dylibs next to the executable in the `.app` (`tauri.macos-bundle.conf.json`, generated by CI). To run from source, `export FOUNDRY_LOCAL_RUNTIME_VERSION=2.3.1` before `npm run tauri dev`. Alternatively, with a Homebrew `foundrylocal` install, point `FOUNDRY_LOCAL_LIB_DIR` at its `libexec` directory (the dylibs live there); without either, the runtime is missing: the header shows `AI unavailable: Foundry runtime failed to load…`, *Start* stays disabled, and notes typed anyway are kept as written (shown as *Filed*, never expanded).
* **Windows:** no Foundry install needed; the SDK downloads its native runtime (including WinML) at build time, and the installer bundles it (`src-tauri/tauri.windows-bundle.conf.json`, staged by CI; for a local installer copy the `*.dll` files next to `foundry_local.dll` in `src-tauri/target/**/out` into `src-tauri/native/` and run `npm run tauri build -- --config src-tauri/tauri.windows-bundle.conf.json`). To run from source, set `FOUNDRY_LOCAL_RUNTIME_VERSION=2.3.1` first (PowerShell: `$env:FOUNDRY_LOCAL_RUNTIME_VERSION="2.3.1"`); the SDK's build script only downloads the runtime when it is set, and also for the local installer command above. Instead install: Visual Studio Build Tools (C++ workload), [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on Windows 11), and Rust's `stable-x86_64-pc-windows-msvc` toolchain.

### Models

Models are not bundled or committed. On first launch Syntari downloads whisper-small and phi-4-mini once into the app data dir (`~/Library/Application Support/com.syntari.app/models` on macOS, `%APPDATA%\\com.syntari.app\\models` on Windows). Defaults: whisper-small (~440 MB, speech) and phi-4-mini (~3.7 GB, expansion/insights); expect a ~4 GB first download and ~4 GB RAM while running. This is the only network access Syntari itself performs.

* **Air-gapped / manual install:** set `SYNTARI_MODELS_DIR` to a folder pre-populated with models (e.g. `foundry model download ... --output <dir>` on a connected machine, then copy it over).
* **Updating models:** delete the model folder and relaunch (re-downloads), or replace the files in `SYNTARI_MODELS_DIR`.
* **Dev with your existing models:** `SYNTARI_MODELS_DIR=$PWD/src-tauri/resources/models npm run tauri dev`

### Using the App

* **Ask across meetings:** the box at the top of the Meetings list answers a question from everything you have recorded. It finds matching transcript lines and notes with a local full-text index (word stems, ranked), gives the best nine to the on-device model, and shows the answer with numbered sources. Click a `[n]` or a source line to open that meeting at that moment. It needs the models loaded and no session running. If nothing matches it says so without calling the model.
* **Meetings:** the app opens on a Meetings list grouped by day, with search across titles, summaries, notes and transcripts. *New meeting* opens a blank session; *Open* views a past meeting; *Continue* resumes it in place (new audio, notes and transcript are appended and the timer carries on).
* **Start / Stop** a session from the header (*Continue* on a reopened meeting); the left pad takes your notes, the middle panel structures them, the right drawer shows the live transcript. *← Meetings* returns to the list when no session is running.
* **AI suggestions:** every 30 s the local model proposes decisions, actions, risks and discussion points from the transcript. They appear only in the Notes panel (amber) until you **Accept** or **Dismiss** them; an unaccepted suggestion is held in memory only (not saved, not exported).
* **Summary:** after stopping, open the *Summary* tab and click *Generate summary* (or *Regenerate*). Long transcripts are summarised in segments and merged. It is saved with the meeting.
* **Pause / Resume:** *Pause* appears next to *Stop* while recording. Capture stops at once (audio already buffered is still transcribed) and nothing is kept until *Resume*. The paused time is removed from the meeting clock, so the timer, note timestamps and saved audio all skip it.
* **Microphone:** the dropdown in the header picks an input device (default: follow the system default). It applies from the next *Start*, and the app rebuilds the stream if that device (or the default) changes mid-meeting, after the change has settled for 300 ms. If the chosen mic is unplugged it falls back to the default.
* **Auto-stop on silence:** after 5 minutes with no sound on either track a banner offers *Keep recording*; otherwise the meeting stops a minute later. Pausing disables it.
* **Save audio (opt-in):** tick *Save audio* before *Start* to write the raw tracks to disk while recording, so a crash loses at most the window being captured. A meeting with saved audio shows *Re-transcribe* (replaces the transcript with a fresh one from the audio; the old transcript is kept if anything fails) and *Delete audio*. *Continue* appends after the saved audio, so the timer may jump forward by any silence that ended the earlier part.
* **Recover from audio:** on the Meetings list, a meeting that never recorded an end (the app quit or crashed) whose saved audio runs more than 30 s past its last transcript line gets an amber *Recover from audio* button. Each row shows its saved-audio size, and *Delete audio older than 30 days* frees space (transcripts and notes stay).
* **Export:** *Copy Markdown* copies the summary (if generated) and your accepted notes to the clipboard; *Export .md* saves the same content, plus the full transcript (speaker, time, and a marker on lines excluded from the summary), to `Downloads` in your home folder (`%USERPROFILE%` on Windows) and reveals the file in Finder / Explorer. Export and backup have no save dialog; they always write to that Downloads folder.
* **Moving and editing notes:** hover a note and use its dropdown to move it to another card; double-click a note to edit its text (Enter saves, Esc cancels). An edit is never overwritten by a late model answer.
* **Renaming meetings:** use *Rename* on a Meetings row; leave it empty to go back to the automatic label.
* **Action items and citations:** action items have a checkbox (saved, struck through when done, exported as `- [x]` / `- [ ]`). Hover a note to see the transcript quote it was grounded in; click it to jump to that spot in the transcript drawer, which can be collapsed. The quote and its timestamp are saved, so the jump also works after reopening a meeting (notes saved before migration 004 show the quote but cannot jump).
* **Pad badges:** each note on the left pad shows *Pending* (model working), *Enhanced* (expanded), *Filed* (categorised, text unchanged) or *Failed* (inference error).
* **Header buttons:** *← Meetings*, *Copy Markdown*, *Export .md*, *Back up DB* and *Logs* appear only while no session is recording. *Pause* and the silence banner are the extra controls shown during recording (the mic picker is locked until you stop).
* **Status pill:** shows one-time model download progress and model errors. *Start* stays disabled until the models are loaded.

### Running Locally

```bash
# Install frontend dependencies
npm install

# Run checks
npm run tsc
cd src-tauri && cargo check && cd ..

# Launch desktop app in development (set FOUNDRY_LOCAL_RUNTIME_VERSION=2.3.1 first, see Prerequisites)
npm run tauri dev
```

---

## 8. Known Limitations & Troubleshooting

* **First launch is slow:** ~4 GB of models download once (progress in the header). The app first checks for free disk (~1 GB for whisper-small, ~4.2 GB for phi-4-mini) and stops with a clear message if there is less. The first launch hashes all weights; later launches re-hash only files whose size or modification time changed, so a normal start is near-instant. While it runs the header shows "Verifying local models…" and Start stays disabled. Transcription is unavailable until it finishes.
* **Transcript accuracy:** speech is cut on silence gaps (3 s minimum, 12 s maximum) and transcribed by whisper-small. Larger models (`whisper-medium`, `whisper-large-v3-turbo`) are in the Foundry catalog but not wired up. Non-speech tags like `(soft music)` are discarded.
* **You-track during remote audio:** the mic track is kept while system audio plays only if its energy after echo cancellation exceeds 0.005 RMS (`audio/mod.rs`). The threshold (`BLEED_RMS`) is untuned; each window logs `[Bleed] mic residual RMS` to help tune it. Speaker bleed may be transcribed as "You" or quiet speech dropped. Use headphones for best results.
* **Echo-line drop:** on speakers, a mic line is dropped only if (a) the 10 ms loudness of the mic followed the system track (correlation above 0.6, up to 300 ms late) in at least 40% of the windows where both carried sound, and (b) the line shares at least 60% of its words (80% for 6 or 7 words) with the remote side's last 20 s. Lines under 6 words are never dropped. With headphones nothing is dropped. The thresholds are untuned; each drop logs `[Echo]`.
* **Silence gap fill:** when the system track delivers nothing (WASAPI sends nothing while nothing plays) it is padded with silence once it falls 500 ms behind the wall clock, keeping 100 ms of slack, so transcript and note times stay aligned. If audio arrives very late a little extra silence can slip in.
* **Whisper's invented lines:** a chunk that is only a stock phrase ("thank you", "thanks for watching", "bye", and similar) is discarded when the audio is faint (RMS under 0.02); the same words spoken clearly are kept. There is no language lock: the Foundry audio API has no language parameter.
* **Saved audio and recovery:** recording is uncompressed PCM (Opus would need an encoder dependency). Recovery is offered automatically only for meetings with no recorded end; for one that ended normally, open it and use *Re-transcribe*. Re-transcribing runs the same ducking, echo cancellation and echo drop as live capture but cannot reproduce the exact live result. Saved audio is not part of *Back up DB*.
* **Ask across meetings:** retrieval is keyword-based (FTS5 with word stemming), so a question that shares no words with the meeting (a paraphrase) finds nothing; phrase it with the words people used. Only the top ten transcript lines, five notes and two summaries reach the model, so broad questions ("summarise everything about X") get partial answers. phi-4-mini can still misstate a passage: check the cited source. Never run `VACUUM` on the live database in place: the index is keyed by row id, which an in-place vacuum can renumber (*Back up DB* uses `VACUUM INTO` and is safe).
* **Exit behaviour:** Foundry's native runtime aborts in a C++ destructor on normal process exit (macOS "quit unexpectedly" dialog). Syntari stops the session and then exits with `_exit(0)` (macOS/Linux) or `TerminateProcess` (Windows) to skip it.
* **Foundry runtime is closed source:** it may make its own telemetry/metadata requests; verify with `scripts/check-egress.sh` or a network monitor.
* **Foundry licensing (reviewed, not legal advice):** the `foundry-local-sdk` crate and the `Microsoft.AI.Foundry.Local.Runtime` NuGet package (the native `foundry_local` library plus ONNX Runtime / GenAI, which the build downloads for Windows and macOS) are **MIT**, so bundling them in an installer is permitted if you ship the MIT notice and `ThirdPartyNotices` from the package. The **Foundry Local CLI** installed via `brew`/`winget` is under separate Microsoft terms that forbid combining it with your application for others to use, so do not redistribute the CLI or its `libfoundry_local.dylib`. Model weights are fetched from the Foundry catalog at first run, not bundled; whisper-small and phi-4-mini are MIT-licensed upstream, but confirm the catalog entries' terms before bundling weights. The macOS build uses the same SDK-downloaded runtime as Windows (no Homebrew copy).
* **Classification accuracy:** phi-4-mini is a small model; ambiguous or tentative notes may be mis-filed. Use the *Move to* menu.
* **Windows is new and untested on hardware:** it compiles in CI but needs a real run. Differences from macOS: system audio uses WASAPI loopback on the default output device (no permission prompt; capture follows the device's mix format, usually 48 kHz stereo); only the microphone privacy toggle matters (*Settings > Privacy > Microphone*); on exit the process is ended with `TerminateProcess` (the Windows counterpart of `_exit`) to skip Foundry's teardown, so confirm there is no crash dialog on quit.
* **Data is unencrypted at rest:** the database is plain SQLite in the app-data directory (`syntari.db`). Encrypting it would need our own SQLCipher connection, because the SQL plugin cannot pass a key. Use FileVault/BitLocker.
* **Summary quality and speed:** the summary comes from phi-4-mini on-device, so it can miss points or be slow on long meetings. An untitled meeting is labelled with its summary, else its first note. A summary is generated automatically when you stop; continuing a meeting marks it stale, so regenerate it after the new part ends (it is replaced automatically on the next stop).
* **Continue and AI context:** continuing seeds the note-expansion window with the saved transcript, and notes typed while reviewing an opened meeting are grounded in its last 45 s (stamped at the end of the transcript by default; pick an earlier moment with the *Refers to* menu above the input).
* **Expansion quality:** phi-4-mini sometimes returns a vague sentence, picks a wrong category, or inherits a transcription error (e.g. Whisper hearing "FY26" as "F526"). Use `dec:`, `act:` or `risk:` to force the category, double-click to correct text. Each inference prints the raw model reply to the log.
* **Old accepted suggestions:** notes accepted from AI suggestions before migration 002 have no AI flag, so they show in the left pad when reloaded.
* **Deleting is permanent:** *Delete* on a meeting removes its notes and transcript; the ✕ on a note removes just that note. Both ask for confirmation, and neither can be undone (use *Back up DB* first if unsure). Delete is disabled while recording.
* **Save failures are shown:** if a database write fails, a red banner appears in the header (click to dismiss) and capture continues; export the notes with *Export .md* or *Copy Markdown* right away.
* **Summary needs a stopped session:** *Generate summary* is disabled while recording or when the transcript is empty. Stopping generates one automatically.
* **Unsigned builds (no paid certificates):** macOS: right-click the app > *Open*, or run `xattr -dr com.apple.quarantine /Applications/Syntari.app`. Windows: SmartScreen > *More info* > *Run anyway*.
* **Logs & backup:** release builds write `syntari.log` (the *Logs* button opens the log folder; the file can contain note and transcript-derived text, so skim before sharing, and it is reset at launch once over 5 MB; dev builds log to the terminal). *Back up DB* saves a copy of the meetings database to Downloads. The header shows the app version; compare it with the latest GitHub Release to see if you are current (no auto-updater).
* **Microphone denied:** if Start fails because the mic is unavailable, a prompt offers to open the OS microphone privacy page. System-audio denial on macOS shows the amber header banner.
* **Non-f32 microphones:** devices whose default format is 16-bit/32-bit integer are converted to f32 in the capture callback; other formats are rejected with a message.
* **Not implemented:** signed/notarized release builds, an auto-updater (check the version in the header against GitHub Releases), upstream-pinned model hashes.


---

## 9. Platform Support & Change Log

| | macOS | Windows |
| :--- | :--- | :--- |
| Mic capture | cpal | cpal |
| System audio | ScreenCaptureKit (permission prompt) | WASAPI loopback, default output device (no prompt) |
| Foundry runtime | Downloaded by the SDK at build time; bundled in the app | Downloaded by the SDK at build time; bundled in the installer |
| Exit workaround | `_exit(0)` | `TerminateProcess` |
| Status | Primary, tested | Compiles in CI; awaiting hardware test |

**Recent changes**

* **Ask across meetings:** question box on the Meetings page with cited, clickable sources; migration 007 adds FTS5 indexes.
* **Recording robustness:** opt-in crash-safe *Save audio* with *Re-transcribe*, *Recover from audio* and storage controls; pause/resume; microphone picker and device-change rebuild; 5-minute silence auto-stop; echo-line drop gated by loudness correlation; silence gap fill on the system track; stock-phrase filter; *Export .md* includes the transcript; stop now transcribes the final 0.5 s.
* **Editing and startup:** double-click a note to edit it, *Rename* a meeting (new `sessions.title` column, migration 003); *Continue* and review-mode notes now get transcript context; the model integrity check skips re-hashing files whose size and mtime are unchanged; expansion prompt reworked (examples, case-insensitive reply parsing, facts are discussion not decisions); failed notes show *Failed* and unchanged ones *Filed*; transparent app icons; header no longer overlaps on narrow windows.
* **Beta readiness:** macOS now bundles the MIT Foundry runtime like Windows (no Homebrew) and has its own `macos` workflow; the SQLCipher/OpenSSL build dependency is gone (no Perl/NASM needed); real app icons; database-error banner; disk-space check, stale temp-file cleanup and a "Verifying local models…" status; delete meetings and notes; *Back up DB*, a log file and *Logs* button; version in the header; microphone-denied prompt; i16/u16/i32 mic support.
* **Meetings, summary and fixes:** Meetings home screen with search and Continue; on-demand full summary; transcript sorted by time with sticky-bottom scrolling; left pad shows only typed notes; the database now opens as plain SQLite (the earlier keyed URL was rejected, so nothing was being saved) and the keychain code was removed.
* **Windows support:** export path falls back to `USERPROFILE` and reveals the file in Explorer; settings shortcut opens the Windows microphone privacy page; Foundry library name is platform-aware; Windows exit handler; icons so the bundle builds.
* **CI:** `.github/workflows/windows.yml` and `macos.yml` run `tsc`, `vitest`, the frontend build, `cargo check` and `cargo test`. A manual run or a `v*` tag also stages the runtime DLLs, builds the NSIS installer, verifies it contains `foundry_local.dll`, uploads it as an artifact and publishes a pre-release (the macOS workflow does the same with an ad-hoc-signed `.dmg`).
* **Installer bundling:** the MIT Foundry runtime DLLs and license notices ship next to the exe (see licensing in section 8); a failed runtime load shows a clear "reinstall" message.
* **Dependencies:** Tailwind pinned to 3.x (the v4 bump broke the frontend build); `@vitejs/plugin-react` 6 for Vite 8.
* **Earlier:** AI note classification, UI refresh, mic/AEC alignment fix, export to `.md`, model-extracted owner/due for actions, session history, first-run model download.

---

## 10. Testing on Windows

CI (`.github/workflows/windows.yml`) runs `tsc`, `vitest`, the frontend build, `cargo check` and `cargo test` on `windows-latest` for pushes to `main` and every PR. Running it manually (*Actions > windows > Run workflow*) or pushing a `v*` tag also builds the installer, checks it contains `foundry_local.dll`, smoke-tests a silent install, attaches it as the `syntari-windows` artifact and publishes a pre-release.

**Tester steps (from source):**

1. Install the Windows prerequisites in section 7, then `git clone` the repo and `git checkout main`.
2. In PowerShell set `$env:FOUNDRY_LOCAL_RUNTIME_VERSION="2.3.1"`, then `npm install` and `npm run tauri dev`. The first Rust build is long (Foundry runtime download).
3. Wait for the ~4 GB model download (header status pill) before starting a session.
4. Run this checklist and report pass/fail with the console output:
   * Mic only: *Start*, speak, transcript appears as "You".
   * System audio: play a YouTube video through speakers/headphones; remote speech appears in the transcript. Try a different default output device and Bluetooth headphones.
   * Notes: type shorthand (e.g. `act:`), check classification and AI suggestions; double-click a note to edit it and *Rename* a meeting.
   * Stop, click *Generate summary*, go to *← Meetings*, open the session, and restart the app to confirm it persists; try *Continue* on it.
   * *Export .md* writes to `%USERPROFILE%\Downloads` and Explorer opens with the file selected.
   * Speaker bleed: play audio through speakers while silent; no "You" lines should appear. Otherwise copy the `[Bleed] mic residual RMS` lines from the log and tune `BLEED_RMS` in `audio/mod.rs`.
   * Try a USB headset whose default format is 16-bit (handled by the I16/U16 conversion in `audio/mod.rs`).
   * Quit the app: note whether a crash dialog appears.
   * Ask: record two short meetings mentioning a price and a name, then ask "what was the price?" from the Meetings page; the answer should cite a source, and clicking it should open that meeting at that line. Ask about something never said and check it answers that it found nothing.
   * Pause: *Pause*, wait a minute, *Resume*; the timer and note times must skip the pause, and speech just before pausing must still appear.
   * Save audio: tick it, record, then kill the app mid-meeting. On the Meetings list the meeting should show *Recover from audio*; run it and check the transcript fills in. Also try *Delete audio older than 30 days* and the size shown per row.
   * Microphone: pick a USB mic in the dropdown, start, unplug it; recording should continue on the default mic (log line `[Audio] default microphone changed`).
   * Silence: record 5 minutes of quiet; the banner should appear, then the meeting stops a minute later.
   * Echo on speakers: play a talk through the speakers with the mic on; the transcript should not repeat the speech as "You" (check the `[Echo]` log lines).
   * With a mic privacy toggle off, the *open settings* hint opens the Windows Microphone privacy page.
5. Optional: capture network connections during use (Resource Monitor or Wireshark) to confirm the only traffic is the model download.

**Tester steps (from CI artifact):** download `syntari-windows` from the workflow run and install the `.exe`. Unsigned, so SmartScreen will warn (*More info > Run anyway*).

---

## License

Syntari is released under the [MIT License](LICENSE). Bundled third-party components keep their own licenses (see `src-tauri/licenses`). See also [SECURITY.md](SECURITY.md) and [CONTRIBUTING.md](CONTRIBUTING.md).
