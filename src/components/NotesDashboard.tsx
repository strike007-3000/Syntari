import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useStore } from "../store";
import { Note, NoteKind, TranscriptChunk, fmtClock } from "../types";
import { notify } from "./ui";

const SECTIONS: { kind: NoteKind; title: string; dot: string }[] = [
  { kind: "decision", title: "Decisions", dot: "bg-emerald-400" },
  { kind: "action", title: "Action Items", dot: "bg-accent" },
  { kind: "risk", title: "Risks & Blockers", dot: "bg-rose-400" },
  { kind: "discussion", title: "Discussion Points", dot: "bg-amber-400" },
];

function Item({ n }: { n: Note }) {
  const { toggleDone, inspect, acceptSuggestion, dismissSuggestion, setKind, deleteNote, editNote, transcript } = useStore();
  const [editing, setEditing] = useState(false);
  const [flipUp, setFlipUp] = useState(false); // quote tooltip opens upward near the bottom edge so it isn't clipped by the scroll area
  const body = n.expanded ?? n.raw;

  if (n.status === "suggested") {
    return (
      <li className="flex flex-wrap items-center gap-2 rounded border border-amber-500/30 bg-amber-950/20 p-2">
        <span className="min-w-[12rem] flex-1 text-sm text-slate-200">{body}</span>
        <span className="mr-1 text-xs text-amber-400">AI suggestion</span>
        <button onClick={() => acceptSuggestion(n.id)} className="rounded px-2.5 py-1 text-xs bg-accent/20 text-accent hover:bg-accent/40">Accept</button>
        <button onClick={() => dismissSuggestion(n.id)} className="rounded px-2.5 py-1 text-xs text-slate-400 hover:text-slate-200">Dismiss</button>
      </li>
    );
  }

  return (
    <li
      className="group relative flex gap-2 rounded p-2 hover:bg-ink-800"
      onMouseEnter={(e) => setFlipUp(e.currentTarget.getBoundingClientRect().bottom + 280 > window.innerHeight)}
    >
      {n.kind === "action" && (
        <input type="checkbox" checked={!!n.done} onChange={() => toggleDone(n.id)} className="mt-1" />
      )}
      {editing ? (
        <input
          autoFocus
          defaultValue={body}
          onBlur={(e) => { const v = e.target.value.trim(); if (v && v !== body) editNote(n.id, v); setEditing(false); }}
          onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") setEditing(false); }}
          className="flex-1 rounded border border-ink-700 bg-ink-900 px-2 py-0.5 text-sm"
        />
      ) : (
      <button
        className="flex-1 text-left"
        onDoubleClick={() => setEditing(true)}
        onClick={() => {
          if (n.citation?.start === undefined) return;
          // citations carry seconds, not chunk ids: jump to the nearest chunk by timestamp
          const t = n.citation.start * 1000;
          const nearest = transcript.reduce<TranscriptChunk | null>((b, c) => (!b || Math.abs(c.ts - t) < Math.abs(b.ts - t) ? c : b), null);
          inspect(nearest?.id ?? null);
        }}
        title="Inspect in transcript"
      >
        <span className={n.done ? "line-through opacity-50" : ""}>{body}</span>
        {n.kind === "action" && (n.owner || n.due) && (
          <span className="ml-2 inline-flex gap-1 align-middle text-xs">{n.owner && <span className="rounded-full bg-accent/15 px-2 text-accent">{n.owner}</span>}{n.due && <span className="rounded-full bg-ink-700 px-2 text-slate-300">due {n.due}</span>}</span>
        )}
        <span className="ml-2 font-mono text-xs text-slate-400">[{fmtClock(n.ts)}]</span>
        {n.status === "pending" && <span className="ml-2 text-xs text-amber-400/80 animate-pulse">expanding…</span>}
        {n.status === "failed" && <span className="ml-2 text-xs text-rose-400/80">(failed)</span>}
      </button>
      )}
      <select
        aria-label="Move to"
        value={n.kind}
        onChange={(e) => setKind(n.id, e.target.value as NoteKind)}
        className="self-start rounded border border-ink-700 bg-ink-800 px-1 py-0.5 text-xs text-slate-400 opacity-0 focus:opacity-100 group-hover:opacity-100"
      >
        {SECTIONS.map((s) => <option key={s.kind} value={s.kind}>{s.title}</option>)}
      </select>
      <button
        aria-label="Delete note"
        onClick={() => confirm("Delete this note?") && deleteNote(n.id)}
        className="flex h-6 w-6 shrink-0 items-center justify-center self-start rounded text-xs text-rose-400 opacity-0 hover:bg-rose-500/10 focus:opacity-100 group-hover:opacity-100"
      >✕</button>
      {n.citation?.quote && (
        <div className={`pointer-events-none absolute left-6 ${flipUp ? "bottom-full" : "top-full"} z-10 hidden w-96 rounded border border-ink-700 bg-ink-950 p-2 text-xs italic shadow-lg group-hover:block`}>
          "{n.citation.quote}"
        </div>
      )}
    </li>
  );
}

export const toMarkdown = (notes: Note[], summary = "", stale = false, transcript: TranscriptChunk[] = []) =>
  [summary && `## Summary${stale ? " (out of date: excluded transcript lines changed, regenerate)" : ""}\n${summary}`, ...SECTIONS.map(({ kind, title }) => {
    const items = notes.filter((n) => n.kind === kind && n.status !== "suggested");
    if (!items.length) return "";
    const lines = items.map((n) =>
      kind === "action"
        ? `- [${n.done ? "x" : " "}] ${n.expanded ?? n.raw}${n.owner ? ` — **${n.owner}**` : ""}${n.due ? ` (due ${n.due})` : ""}`
        : `- ${n.expanded ?? n.raw} _[${fmtClock(n.ts)}]_`,
    );
    return `## ${title}\n${lines.join("\n")}`;
  }), transcript.length && `## Transcript\n${transcript.map((c) => `**${c.speaker}** _[${fmtClock(c.ts)}]_ ${c.text}${c.excluded ? " _(excluded from summary)_" : ""}`).join("\n\n")}`].filter(Boolean).join("\n\n");

// elapsed time and part counter so a slow on-device summary doesn't look like a hang
function useSummarizeStatus(active: boolean) {
  const [secs, setSecs] = useState(0);
  const [part, setPart] = useState<{ part: number; total: number } | null>(null);
  // listener lives for the component's lifetime so it is registered before summarize() invokes the backend
  useEffect(() => {
    const sub = listen<{ part: number; total: number }>("summarize-progress", (e) => setPart(e.payload));
    return () => { sub.then((off) => off()); };
  }, []);
  useEffect(() => {
    if (!active) return;
    setSecs(0); setPart(null);
    const t = setInterval(() => setSecs((s) => s + 1), 1000);
    return () => clearInterval(t);
  }, [active]);
  return active ? `${part && part.total > 1 ? `part ${part.part}/${part.total}, ` : ""}${secs}s` : "";
}

export default function NotesDashboard() {
  const { notes, view, setView, summary, summarizing, summaryStale, summarize, transcript, running } = useStore();
  const status = useSummarizeStatus(summarizing);
  return (
    <section className="flex h-full flex-col" aria-label="Notes view">
      <nav className="flex h-11 shrink-0 items-center gap-1 border-b border-ink-700 px-2 text-sm">
        {(["notes", "summary", "markdown"] as const).map((v) => (
          <button key={v} onClick={() => setView(v)} className={`rounded-md px-3 py-1 transition-colors ${view === v ? "bg-ink-700 text-white font-medium" : "text-slate-400 hover:text-slate-200"}`}>
            {v === "markdown" ? "Preview" : v === "summary" ? "Summary" : "Notes"}
          </button>
        ))}
      </nav>
      <div className="flex-1 overflow-y-auto p-3">
        {view === "notes" && notes.length === 0 && (
          <div className="mx-auto mt-16 max-w-sm text-center text-sm text-slate-400">
            <p className="mb-1 font-medium text-slate-200">No notes yet</p>
            Jot a note on the left or start transcribing. Decisions, action items, risks and discussion points are filed here as they come up.
          </div>
        )}
        {view === "notes" && notes.length > 0 &&
          SECTIONS.map(({ kind, title, dot }) => {
            const items = notes.filter((n) => n.kind === kind);
            return (
              <div key={kind} className={`mb-3 rounded-lg border border-ink-700 bg-ink-900 ${items.length ? "p-3" : "px-3 py-2"}`}>
                <h2 className={`flex items-center gap-2 text-sm font-semibold ${items.length ? "mb-1 text-slate-100" : "text-slate-400"}`}>
                  <span className={`h-2 w-2 rounded-full ${dot}`} />{title}
                  {items.length > 0 && <span className="rounded-full bg-ink-700 px-1.5 text-xs font-normal text-slate-400">{items.length}</span>}
                </h2>
                {items.length ? <ul>{items.map((n) => <Item key={n.id} n={n} />)}</ul> : <span className="sr-only">None yet</span>}
              </div>
            );
          })}
        {view === "summary" && (
          <div className="space-y-3 text-sm">
            <button
              onClick={() => summarize().catch(notify)}
              disabled={running || summarizing || !transcript.some((c) => !c.excluded)}
              title={running ? "Stop the session first" : undefined}
              className="rounded-md bg-accent px-3 py-1.5 text-xs font-semibold text-ink-950 disabled:opacity-50"
            >
              {summarizing ? `Summarizing… (${status})` : summary ? "Regenerate summary" : "Generate summary"}
            </button>
            {summary ? (
              <article className="space-y-2 [&_h2]:mb-1 [&_h2]:mt-4 [&_h2]:text-base [&_h2]:font-semibold [&_h2]:text-slate-100 [&_h3]:mb-1 [&_h3]:mt-3 [&_h3]:text-sm [&_h3]:font-semibold [&_h3]:text-slate-100 [&_ul]:list-disc [&_ul]:pl-5 [&_.contains-task-list]:list-none [&_.contains-task-list]:pl-0 [&_input]:mr-2 [&_em]:text-slate-400">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{summary}</ReactMarkdown>
              </article>
            ) : (
              <p className="text-xs text-slate-400">Stop the session, then generate a full summary of the transcript.</p>
            )}
            {summaryStale && summary && !summarizing && <p className="text-xs text-amber-400">Excluded lines changed since this summary was written. Regenerate to update it.</p>}
            {summarizing && <p className="text-xs text-slate-400">Running on your device; long meetings can take a few minutes.</p>}
            {!running && transcript.some((c) => c.excluded) && <p className="text-xs text-slate-400">{transcript.filter((c) => c.excluded).length} transcript line(s) excluded from the summary.</p>}
          </div>
        )}
        {view === "markdown" && (
          <article className="space-y-2 text-sm [&_h2]:mb-1 [&_h2]:mt-4 [&_h2]:text-base [&_h2]:font-semibold [&_h2]:text-slate-100 [&_h3]:mb-1 [&_h3]:mt-3 [&_h3]:text-sm [&_h3]:font-semibold [&_h3]:text-slate-100 [&_ul]:list-disc [&_ul]:pl-5 [&_.contains-task-list]:list-none [&_.contains-task-list]:pl-0 [&_input]:mr-2 [&_em]:text-slate-400">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{toMarkdown(notes, summary, summaryStale)}</ReactMarkdown>
          </article>
        )}
      </div>
    </section>
  );
}
