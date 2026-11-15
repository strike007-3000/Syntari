import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore, MeetingRow } from "../store";

const dayLabel = (ms: number) => {
  const d = new Date(ms), today = new Date();
  const diff = Math.round((new Date(today.toDateString()).getTime() - new Date(d.toDateString()).getTime()) / 864e5);
  const full = d.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" });
  return diff === 0 ? `Today • ${full}` : diff === 1 ? `Yesterday • ${full}` : full;
};

export default function Meetings() {
  const { listSessions, loadSession, newMeeting, running, deleteSession, renameSession, modelMsg } = useStore();
  const [renaming, setRenaming] = useState<number | null>(null);
  // An in-flight rename must settle before any button navigates away; a failed save blocks the action so the typed title isn't lost.
  const saving = useRef<Promise<boolean>>(Promise.resolve(true));
  const afterSave = (act: () => unknown) => void saving.current.then((ok) => ok && act());
  const [q, setQ] = useState("");
  const [rows, setRows] = useState<MeetingRow[]>([]);
  // saved audio by meeting: bytes, seconds of audio, days since last written
  const [recs, setRecs] = useState<Record<number, [number, number, number]>>({});
  const refreshRecs = () => invoke<[number, number, number, number][]>("list_recordings").then((l) => setRecs(Object.fromEntries(l.map(([id, b, s, a]) => [id, [b, s, a]])))).catch(() => setRecs({}));
  const refresh = () => { listSessions(q).then(setRows).catch(() => setRows([])); refreshRecs(); };
  const totalMb = Object.values(recs).reduce((n, r) => n + r[0], 0) / 1e6;
  const mb = (id: number) => `${(recs[id][0] / 1e6).toFixed(0)} MB`;
  const interrupted = (r: MeetingRow) => !!recs[r.id] && r.ended_at === null && recs[r.id][1] - (r.last_ts ?? 0) / 1000 > 30;
  useEffect(() => { listSessions(q).then(setRows).catch(() => setRows([])); }, [q, listSessions]); // typing searches only the database
  useEffect(() => { refreshRecs(); }, []);
  const remove = (id: number) => {
    if (confirm("Delete this meeting, its notes and its transcript? This cannot be undone."))
      deleteSession(id).then(refresh).catch((e) => alert(`Delete failed: ${e}`));
  };

  const groups = rows.reduce<Record<string, MeetingRow[]>>((g, r) => ((g[dayLabel(r.started_at)] ??= []).push(r), g), {});
  const open = (id: number) => loadSession(id).catch((e) => alert(String(e)));

  return (
    <div className="flex h-full flex-col overflow-y-auto p-8">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Meetings</h1>
        {modelMsg && <span className="rounded-full border border-amber-500/30 bg-amber-500/15 px-2.5 py-0.5 text-xs text-amber-400">{modelMsg}</span>}
        <button onClick={() => afterSave(newMeeting)} disabled={running} className="rounded-md bg-accent px-4 py-1.5 text-sm font-semibold text-ink-950 disabled:opacity-50">New meeting</button>
      </div>
      {totalMb > 0 && (
        <div className="mb-3 flex items-center gap-3 text-xs text-slate-400">
          Saved audio: {totalMb.toFixed(0)} MB
          <button
            className="rounded-md border border-ink-700 px-2 py-0.5 hover:text-white"
            onClick={() => confirm("Delete saved audio not written to for 30 days? Transcripts and notes stay.") && invoke<number>("delete_old_recordings", { days: 30 }).then((b) => { alert(`Freed ${(b / 1e6).toFixed(0)} MB`); refresh(); }).catch((e) => alert(String(e)))}
          >Delete audio older than 30 days</button>
        </div>
      )}
      <input
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder="Search summaries, notes and transcripts"
        className="mb-6 w-full rounded-lg border border-ink-700 bg-ink-900 px-4 py-2 text-sm placeholder-slate-500 focus:outline-none"
      />
      {rows.length === 0 && <p className="text-sm text-slate-500">{q ? "No meetings match." : "No meetings yet. Start a new one."}</p>}
      {Object.entries(groups).map(([day, items]) => (
        <section key={day} className="mb-6">
          <h2 className="mb-2 text-sm font-semibold text-slate-300">{day}</h2>
          <ul className="space-y-2">
            {items.map((r) => (
              <li key={r.id} className="flex items-center gap-4 rounded-lg border border-ink-700 bg-ink-900 p-3">
                <span className="font-mono text-xs text-slate-400">{new Date(r.started_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</span>
                {renaming === r.id ? (
                  <input
                    autoFocus
                    defaultValue={r.title ?? ""}
                    placeholder="Meeting title"
                    onBlur={(e) => {
                      const el = e.target;
                      saving.current = renameSession(r.id, el.value)
                        .then(() => { setRenaming(null); refresh(); return true; })
                        .catch((err) => { alert(`Rename failed: ${err}`); el.focus(); return false; });
                    }}
                    onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") setRenaming(null); }}
                    className="min-w-0 flex-1 rounded border border-ink-700 bg-ink-950 px-2 py-0.5 text-sm"
                  />
                ) : (
                  <button onClick={() => afterSave(() => open(r.id))} className="min-w-0 flex-1 text-left">
                    <span className="line-clamp-2 text-sm">{r.title || r.summary?.replace(/[#*\-]/g, "").trim() || r.first_note || "No summary yet"}</span>
                  </button>
                )}
                {interrupted(r) && (
                  <button
                    disabled={running}
                    onClick={() => confirm("This meeting's recording runs past its transcript (it was probably interrupted). Rebuild the transcript from the saved audio?") && loadSession(r.id).then(() => { if (useStore.getState().sessionId !== r.id) throw new Error("could not open the meeting"); return useStore.getState().retranscribe(); }).then(refresh).catch((e) => alert(`Re-transcribe failed: ${e}`))}
                    className="rounded-md bg-amber-500/15 px-2 py-1 text-xs text-amber-400 disabled:opacity-50"
                    title="Recording found from an interrupted meeting"
                  >Recover from audio</button>
                )}
                {recs[r.id] && <span className="text-xs text-slate-500" title="Saved audio">♪ {mb(r.id)}</span>}
                <button onClick={() => setRenaming(r.id)} className="rounded-md border border-ink-700 px-3 py-1 text-xs hover:text-white">Rename</button>
                <button onClick={() => afterSave(() => open(r.id))} className="rounded-md border border-ink-700 px-3 py-1 text-xs hover:text-white">Open</button>
                <button
                  onClick={() => afterSave(() => open(r.id).then(() => useStore.getState().start()).catch((e) => alert(String(e))))}
                  disabled={running}
                  className="rounded-md bg-accent/20 px-3 py-1 text-xs font-semibold text-accent hover:bg-accent/30 disabled:opacity-50"
                >Continue</button>
                <button onClick={() => afterSave(() => remove(r.id))} disabled={running} className="rounded-md px-2 py-1 text-xs text-rose-400 hover:bg-rose-500/10 disabled:opacity-50" aria-label="Delete meeting">Delete</button>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
