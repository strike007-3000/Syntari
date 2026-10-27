import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useStore } from "../store";
import { Note, NoteKind, TranscriptChunk, fmtClock } from "../types";

const SECTIONS: { kind: NoteKind; title: string; dot: string }[] = [
  { kind: "decision", title: "Decisions", dot: "bg-emerald-400" },
  { kind: "action", title: "Action Items", dot: "bg-accent" },
  { kind: "risk", title: "Risks & Blockers", dot: "bg-rose-400" },
  { kind: "discussion", title: "Discussion Points", dot: "bg-amber-400" },
];

function Item({ n }: { n: Note }) {
  const { toggleDone, inspect, acceptSuggestion, dismissSuggestion, setKind, transcript } = useStore();
  const body = n.expanded ?? n.raw;

  if (n.status === "suggested") {
    return (
      <li className="flex gap-2 rounded border border-amber-500/30 bg-amber-950/20 p-2">
        <span className="flex-1 text-sm opacity-80">{body}</span>
        <span className="text-xs text-amber-400/60 self-center mr-1">AI suggestion</span>
        <button onClick={() => acceptSuggestion(n.id)} className="rounded px-2 py-0.5 text-xs bg-accent/20 text-accent hover:bg-accent/40">Accept</button>
        <button onClick={() => dismissSuggestion(n.id)} className="rounded px-2 py-0.5 text-xs text-slate-400 hover:text-slate-200">Dismiss</button>
      </li>
    );
  }

  return (
    <li className="group relative flex gap-2 rounded p-2 hover:bg-ink-800">
      {n.kind === "action" && (
        <input type="checkbox" checked={!!n.done} onChange={() => toggleDone(n.id)} className="mt-1" />
      )}
      <button
        className="flex-1 text-left"
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
          <span className="ml-2 text-xs text-accent">{n.owner} {n.due && `· ${n.due}`}</span>
        )}
        <span className="ml-2 font-mono text-xs text-slate-500">[{fmtClock(n.ts)}]</span>
        {n.status === "pending" && <span className="ml-2 text-xs text-amber-400/80 animate-pulse">expanding…</span>}
        {n.status === "failed" && <span className="ml-2 text-xs text-rose-400/80">(failed)</span>}
      </button>
      <select
        aria-label="Move to"
        value={n.kind}
        onChange={(e) => setKind(n.id, e.target.value as NoteKind)}
        className="self-start rounded border border-ink-700 bg-ink-800 px-1 py-0.5 text-xs text-slate-400 opacity-0 focus:opacity-100 group-hover:opacity-100"
      >
        {SECTIONS.map((s) => <option key={s.kind} value={s.kind}>{s.title}</option>)}
      </select>
      {n.citation?.quote && (
        <div className="pointer-events-none absolute left-6 top-full z-10 hidden w-96 rounded border border-ink-700 bg-ink-950 p-2 text-xs italic shadow-lg group-hover:block">
          "{n.citation.quote}"
        </div>
      )}
    </li>
  );
}

export const toMarkdown = (notes: Note[], summary = "") =>
  [summary && `## Summary\n${summary}`, ...SECTIONS.map(({ kind, title }) => {
    const items = notes.filter((n) => n.kind === kind && n.status !== "suggested");
    if (!items.length) return "";
    const lines = items.map((n) =>
      kind === "action"
        ? `- [${n.done ? "x" : " "}] ${n.expanded ?? n.raw}${n.owner ? ` — **${n.owner}**` : ""}${n.due ? ` (due ${n.due})` : ""}`
        : `- ${n.expanded ?? n.raw} _[${fmtClock(n.ts)}]_`,
    );
    return `## ${title}\n${lines.join("\n")}`;
  })].filter(Boolean).join("\n\n");

export default function NotesDashboard() {
  const { notes, view, setView, summary, summarizing, summarize, transcript, running } = useStore();
  return (
    <section className="flex h-full flex-col" aria-label="Notes view">
      <nav className="flex gap-1 border-b border-ink-700 p-2 text-sm">
        {(["notes", "summary", "markdown"] as const).map((v) => (
          <button key={v} onClick={() => setView(v)} className={`rounded-md px-3 py-1 transition-colors ${view === v ? "bg-ink-700 text-white font-medium" : "text-slate-400 hover:text-slate-200"}`}>
            {v === "markdown" ? "Markdown" : v === "summary" ? "Summary" : "Notes"}
          </button>
        ))}
      </nav>
      <div className="flex-1 overflow-y-auto p-3">
        {view === "notes" &&
          SECTIONS.map(({ kind, title, dot }) => {
            const items = notes.filter((n) => n.kind === kind);
            return (
              <div key={kind} className="mb-3 rounded-lg border border-ink-700 bg-ink-900 p-3">
                <h2 className="mb-1 flex items-center gap-2 text-sm font-semibold text-slate-100">
                  <span className={`h-2 w-2 rounded-full ${dot}`} />{title}
                  {items.length > 0 && <span className="rounded-full bg-ink-700 px-1.5 text-xs font-normal text-slate-400">{items.length}</span>}
                </h2>
                {items.length ? <ul>{items.map((n) => <Item key={n.id} n={n} />)}</ul> : <p className="text-xs text-slate-500">Nothing captured yet. Jot a note or start transcribing.</p>}
              </div>
            );
          })}
        {view === "summary" && (
          <div className="space-y-3 text-sm">
            <button
              onClick={() => summarize().catch((e) => alert(String(e)))}
              disabled={running || summarizing || !transcript.length}
              title={running ? "Stop the session first" : undefined}
              className="rounded-md bg-accent px-3 py-1.5 text-xs font-semibold text-ink-950 disabled:opacity-50"
            >
              {summarizing ? "Summarizing…" : summary ? "Regenerate summary" : "Generate summary"}
            </button>
            {summary ? (
              <article className="space-y-2 [&_h3]:mt-3 [&_h3]:font-semibold [&_ul]:list-disc [&_ul]:pl-5">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{summary}</ReactMarkdown>
              </article>
            ) : (
              <p className="text-xs text-slate-500">Stop the session, then generate a full summary of the transcript.</p>
            )}
          </div>
        )}
        {view === "markdown" && (
          <article className="space-y-2 text-sm [&_h2]:mt-3 [&_h2]:font-semibold [&_ul]:list-disc [&_ul]:pl-5">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{toMarkdown(notes, summary)}</ReactMarkdown>
          </article>
        )}
      </div>
    </section>
  );
}
