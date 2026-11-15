import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MIC_KEY, RECORD_KEY, useStore } from "../store";
import { AudioLevels } from "../types";

const QUIET = 0.003, WARN_AFTER_S = 300, STOP_AFTER_S = 360;

const btn = "rounded-md border border-ink-700 bg-ink-800 px-2.5 py-1 text-xs text-slate-300 transition-colors hover:text-white disabled:opacity-50";

/** Opt-in crash-safe recording, and for a meeting that has saved audio: transcribe it again, or delete it. */
export default function RecordingControls() {
  const { sessionId, running, retranscribe } = useStore();
  const [on, setOn] = useState(() => localStorage.getItem(RECORD_KEY) === "1");
  const [bytes, setBytes] = useState(0);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (sessionId === null || running) return setBytes(0);
    invoke<number>("recording_size", { sessionId }).then(setBytes).catch(() => setBytes(0));
  }, [sessionId, running, busy]);
  const mics = useMics();
  const [quiet, keep] = useQuietSeconds();
  const { stop } = useStore();
  const stopping = useRef(false);
  useEffect(() => {
    if (quiet >= STOP_AFTER_S && !stopping.current) {
      stopping.current = true; // stop() waits for the last transcription; ask once
      stop().catch(() => {}).finally(() => { stopping.current = false; });
    }
  }, [quiet, stop]);
  return (
    <>
      {quiet >= WARN_AFTER_S && (
        <span className="rounded-full border border-amber-500/30 bg-amber-500/15 px-2.5 py-0.5 text-xs text-amber-400">
          No audio for {Math.floor(quiet / 60)} min: stopping in {Math.max(0, STOP_AFTER_S - quiet)} s
          <button onClick={keep} className="ml-2 underline">Keep recording</button>
        </span>
      )}
      <select
        aria-label="Microphone"
        disabled={running}
        defaultValue={localStorage.getItem(MIC_KEY) ?? ""}
        onChange={(e) => localStorage.setItem(MIC_KEY, e.target.value)}
        className="max-w-[9rem] rounded border border-ink-700 bg-ink-800 px-1 py-0.5 text-xs text-slate-300 disabled:opacity-50"
        title="Microphone to record. Applies from the next Start."
      >
        <option value="">Default microphone</option>
        {mics.map((m) => <option key={m} value={m}>{m}</option>)}
      </select>
      <label className="flex items-center gap-1 text-xs text-slate-400" title="Writes raw audio to this computer's app data folder while recording, so a crash loses almost nothing and a meeting can be transcribed again. About 115 MB per hour per track.">
        <input type="checkbox" checked={on} disabled={running} onChange={(e) => { localStorage.setItem(RECORD_KEY, e.target.checked ? "1" : "0"); setOn(e.target.checked); }} />
        Save audio
      </label>
      {bytes > 0 && !running && (
        <>
          <button className={btn} disabled={busy} onClick={() => { if (confirm("Replace this meeting's transcript with a fresh one from the saved audio?")) { setBusy(true); retranscribe().catch((e) => alert(`Re-transcribe failed: ${e}`)).finally(() => setBusy(false)); } }}>
            {busy ? "Transcribing…" : `Re-transcribe (${(bytes / 1e6).toFixed(0)} MB)`}
          </button>
          <button className={btn} disabled={busy} onClick={() => confirm("Delete the saved audio for this meeting? The transcript stays.") && invoke("delete_recording", { sessionId }).then(() => setBytes(0))}>Delete audio</button>
        </>
      )}
    </>
  );
}

function useMics() {
  const [mics, setMics] = useState<string[]>([]);
  useEffect(() => { invoke<string[]>("list_mics").then(setMics).catch(() => {}); }, []);
  return mics;
}

/** Seconds since either track last carried sound; 0 while paused or stopped. A keep-recording click is any new sound. */
function useQuietSeconds() {
  const { running, paused } = useStore();
  const [quiet, setQuiet] = useState(0);
  const lastSound = useRef(Date.now());
  useEffect(() => {
    setQuiet(0);
    if (!running || paused) return;
    lastSound.current = Date.now();
    const t = setInterval(() => {
      invoke<AudioLevels>("get_audio_levels").then((l) => {
        if (l.mic > QUIET || l.system > QUIET) lastSound.current = Date.now();
        setQuiet(Math.floor((Date.now() - lastSound.current) / 1000));
      }).catch(() => {});
    }, 1000);
    return () => clearInterval(t);
  }, [running, paused]);
  return [quiet, () => { lastSound.current = Date.now(); setQuiet(0); }] as const;
}
