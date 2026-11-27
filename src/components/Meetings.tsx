import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus } from "lucide-react";
import AskMeetings from "./AskMeetings";
import { useStore, MeetingRow } from "../store";
import { Menu, btn, notify } from "./ui";

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
      deleteSession(id).then(refresh).catch((e) => notify(`Delete failed: ${e}`));
  };

  const groups = rows.reduce<Record<string, MeetingRow[]>>((g, r) => ((g[dayLabel(r.started_at)] ??= []).push(r), g), {});
  const open = (id: number) => loadSession(id).catch(notify);

  return (
    <div className="flex h-full flex-col overflow-y-auto">
      {modelMsg && (
        <div className="border-b border-amber-500/30 bg-amber-500/10 px-8 py-1.5 text-xs font-medium text-amber-400" title="One-time download of the on-device models; recording unlocks when it finishes">{modelMsg}</div>
      )}
      <div className="mx-auto w-full max-w-4xl flex-1 px-8 py-8">
      <div className="mb-6 flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold">Meetings</h1>
          <p className="mt-0.5 text-sm text-slate-400">Everything stays on this device.</p>
        </div>
        <button onClick={() => afterSave(newMeeting)} disabled={running} className={btn.primary}><Plus size={14} /> New meeting</button>
      </div>
      <AskMeetings onSearch={setQ} />
      {rows.length === 0 && (q ? (
        <p className="py-8 text-center text-sm text-slate-400">No meetings match “{q}”.</p>
      ) : (
        <div className="rounded-xl border border-dashed border-ink-700 py-12 text-center">
          <p className="font-medium text-slate-200">No meetings yet</p>
          <p className="mb-4 mt-1 text-sm text-slate-400">Start one and Syntari transcribes it and files your notes as you go.</p>
          <button onClick={() => afterSave(newMeeting)} disabled={running} className={btn.primary}><Plus size={14} /> New meeting</button>
        </div>
      ))}
      {Object.entries(groups).map(([day, items]) => (
        <section key={day} className="mb-6">
          <h2 className="mb-2 text-xs font-semibold uppercase tracking-wider text-slate-400">{day}</h2>
          <ul className="space-y-2">
            {items.map((r) => (
              <li key={r.id} className="flex items-center gap-4 rounded-lg border border-ink-700 bg-ink-900 p-3 transition-colors hover:border-slate-600 hover:bg-ink-800">
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
                        .catch((err) => { notify(`Rename failed: ${err}`); el.focus(); return false; });
                    }}
                    onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") setRenaming(null); }}
                    aria-describedby={`rename-hint-${r.id}`}
                    className="min-w-0 flex-1 rounded border border-accent/60 bg-ink-950 px-2 py-1 text-sm outline-none"
                  />
                ) : (
                  <button onClick={() => afterSave(() => open(r.id))} className="min-w-0 flex-1 self-stretch text-left" title="Open meeting">
                    <span className={`line-clamp-2 text-sm ${r.title || r.summary || r.first_note ? "" : "italic text-slate-400"}`}>{r.title || r.summary?.replace(/[#*\-]/g, " ").replace(/\s+/g, " ").trim() || r.first_note || "Untitled meeting"}</span>
                  </button>
                )}
                {renaming === r.id && <span id={`rename-hint-${r.id}`} className="text-xs text-slate-400">Enter to save · Esc to cancel</span>}
                {interrupted(r) && (
                  <button
                    disabled={running}
                    onClick={() => confirm("This meeting's recording runs past its transcript (it was probably interrupted). Rebuild the transcript from the saved audio?") && loadSession(r.id).then(() => { if (useStore.getState().sessionId !== r.id) throw new Error("could not open the meeting"); return useStore.getState().retranscribe(); }).then(refresh).catch((e) => notify(`Re-transcribe failed: ${e}`))}
                    className="rounded-md bg-amber-500/15 px-2.5 py-1 text-xs font-medium text-amber-400 hover:bg-amber-500/25 disabled:opacity-50"
                    title="Recording found from an interrupted meeting"
                  >Interrupted · Recover</button>
                )}
                {recs[r.id] && <span className="text-xs text-slate-400" title="Saved audio">♪ {mb(r.id)}</span>}
                <button
                  onClick={() => afterSave(() => open(r.id).then(() => useStore.getState().start()).catch(notify))}
                  disabled={running}
                  className="rounded-md bg-accent/20 px-3 py-1 text-xs font-semibold text-accent hover:bg-accent/30 disabled:opacity-50"
                >Continue</button>
                <Menu
                  label="Meeting actions"
                  items={[
                    { label: "Open", onClick: () => afterSave(() => open(r.id)) },
                    { label: "Rename", onClick: () => setRenaming(r.id) },
                    { label: "Delete meeting", danger: true, disabled: running, onClick: () => afterSave(() => remove(r.id)) },
                  ]}
                />
              </li>
            ))}
          </ul>
        </section>
      ))}
      {totalMb > 0 && (
        <footer className="mt-4 flex items-center justify-between gap-3 border-t border-ink-700 pt-4 text-xs text-slate-400">
          <span>Saved audio uses {totalMb.toFixed(0)} MB on this device.</span>
          <button
            className={btn.secondary}
            onClick={() => confirm("Delete saved audio not written to for 30 days? Transcripts and notes stay.") && invoke<number>("delete_old_recordings", { days: 30 }).then((b) => { notify(`Freed ${(b / 1e6).toFixed(0)} MB`); refresh(); }).catch(notify)}
          >Delete audio older than 30 days</button>
        </footer>
      )}
      </div>
    </div>
  );
}
