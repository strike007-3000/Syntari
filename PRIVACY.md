# Privacy Policy

_Last updated: 2026-11-20_

Syntari is a free, open-source (MIT) desktop app. It runs entirely on your device. The maintainers run no servers for it and receive no data from it.

## What Syntari does not do

- It does not collect, transmit, or sell your audio, transcripts, notes, summaries, or any other personal data.
- It has no accounts, analytics, advertising, or telemetry of its own.
- It makes no calls to cloud AI services. Speech recognition and summarization run locally.

## What stays on your device

Everything you create is stored locally in the app's data folder:

- **Meetings, transcripts, notes, summaries:** a plain (unencrypted) SQLite database. Use FileVault or BitLocker for disk encryption.
- **Audio:** held in memory while recording. Short temporary `.wav` slices are written to the OS temp folder during transcription, then overwritten and deleted. If you turn on **Save audio** (off by default), raw audio is kept on disk until you delete it.
- **Logs:** `syntari.log` may contain note and transcript-derived text. It stays on your device.

You can delete meetings, saved audio, and logs at any time from the app or by removing the app data folder. Uninstalling the app does not remove the data folder automatically.

## Network access

- **One-time model download:** on first launch Syntari downloads its speech and language models from the Microsoft Foundry Local catalog. This request does not include your data.
- **Foundry Local runtime:** the closed-source runtime Syntari uses to run models may make its own network requests. Syntari does not control these. You can check with a network monitor or `scripts/check-egress.sh`.
- **Downloads and package managers:** installing through GitHub Releases, Homebrew, or Scoop is subject to those services' own privacy policies.

## Permissions

- **Microphone:** to record your voice.
- **System audio** (macOS Screen Recording / ScreenCaptureKit, Windows loopback): to capture the other side of a call.

Audio is only captured while you are recording.

## Recording others

Recording meetings may require consent from other participants under local law. You are responsible for obtaining it.

## Children

Syntari is not directed at children and collects no data from anyone.

## Changes and contact

Changes to this policy are tracked in this repository's git history. Questions or concerns: email syntari.app@gmail.com or open an issue at https://github.com/strike007-3000/Syntari/issues.
