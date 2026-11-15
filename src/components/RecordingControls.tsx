import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RECORD_KEY, useStore } from "../store";

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
  return (
    <>
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
