import { useEffect, useState } from "react";
import { useStore, MeetingRow } from "../store";

const dayLabel = (ms: number) => {
  const d = new Date(ms), today = new Date();
  const diff = Math.round((new Date(today.toDateString()).getTime() - new Date(d.toDateString()).getTime()) / 864e5);
  const full = d.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" });
  return diff === 0 ? `Today • ${full}` : diff === 1 ? `Yesterday • ${full}` : full;
};

export default function Meetings() {
  const { listSessions, loadSession, newMeeting, running, deleteSession, renameSession } = useStore();
  const [renaming, setRenaming] = useState<number | null>(null);
  const [q, setQ] = useState("");
  const [rows, setRows] = useState<MeetingRow[]>([]);
  const refresh = () => listSessions(q).then(setRows).catch(() => setRows([]));
  useEffect(() => { refresh(); }, [q, listSessions]);
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
        <button onClick={newMeeting} disabled={running} className="rounded-md bg-accent px-4 py-1.5 text-sm font-semibold text-ink-950 disabled:opacity-50">New meeting</button>
      </div>
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
                    onBlur={(e) => { renameSession(r.id, e.target.value).then(refresh); setRenaming(null); }}
                    onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") setRenaming(null); }}
                    className="min-w-0 flex-1 rounded border border-ink-700 bg-ink-950 px-2 py-0.5 text-sm"
                  />
                ) : (
                  <button onClick={() => open(r.id)} onDoubleClick={() => setRenaming(r.id)} title="Double-click to rename" className="min-w-0 flex-1 text-left">
                    <span className="line-clamp-2 text-sm">{r.title || r.summary?.replace(/[#*\-]/g, "").trim() || r.first_note || "No summary yet"}</span>
                  </button>
                )}
                <button onClick={() => open(r.id)} className="rounded-md border border-ink-700 px-3 py-1 text-xs hover:text-white">Open</button>
                <button
                  onClick={() => open(r.id).then(() => useStore.getState().start()).catch((e) => alert(String(e)))}
                  disabled={running}
                  className="rounded-md bg-accent/20 px-3 py-1 text-xs font-semibold text-accent hover:bg-accent/30 disabled:opacity-50"
                >Continue</button>
                <button onClick={() => remove(r.id)} disabled={running} className="rounded-md px-2 py-1 text-xs text-rose-400 hover:bg-rose-500/10 disabled:opacity-50" aria-label="Delete meeting">Delete</button>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
