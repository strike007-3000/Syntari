import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useStore } from "../store";
import { Note, NoteKind, fmtClock } from "../types";

const SECTIONS: { kind: NoteKind; title: string }[] = [
  { kind: "decision", title: "Decisions" },
  { kind: "action", title: "Action Items" },
  { kind: "risk", title: "Risks & Blockers" },
  { kind: "discussion", title: "Discussion Points" },
];

function Item({ n }: { n: Note }) {
  const { toggleDone, inspect } = useStore();
  const body = n.expanded ?? n.raw;
  return (
    <li className="group relative flex gap-2 rounded p-2 hover:bg-ink-800">
      {n.kind === "action" && (
        <input type="checkbox" checked={!!n.done} onChange={() => toggleDone(n.id)} className="mt-1" />
      )}
      <button
        className="flex-1 text-left"
        onClick={() => n.citation && inspect(n.citation.chunkIds[0] ?? null)}
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
      {n.citation?.quote && (
        <div className="pointer-events-none absolute left-6 top-full z-10 hidden w-96 rounded border border-ink-700 bg-ink-950 p-2 text-xs italic shadow-lg group-hover:block">
          “{n.citation.quote}”
        </div>
      )}
    </li>
  );
}

export const toMarkdown = (notes: Note[]) =>
  SECTIONS.map(({ kind, title }) => {
    const items = notes.filter((n) => n.kind === kind);
    if (!items.length) return "";
    const lines = items.map((n) =>
      kind === "action"
        ? `- [${n.done ? "x" : " "}] ${n.expanded ?? n.raw}${n.owner ? ` — **${n.owner}**` : ""}${n.due ? ` (due ${n.due})` : ""}`
        : `- ${n.expanded ?? n.raw} _[${fmtClock(n.ts)}]_`,
    );
    return `## ${title}\n${lines.join("\n")}`;
  }).filter(Boolean).join("\n\n");

export default function ArchitectDashboard() {
  const { notes, view, setView } = useStore();
  return (
    <section className="flex h-full flex-col" aria-label="Architect view">
      <nav className="flex gap-1 border-b border-ink-700 p-2 text-sm">
        {(["architect", "markdown"] as const).map((v) => (
          <button key={v} onClick={() => setView(v)} className={`rounded px-3 py-1 ${view === v ? "bg-ink-700 text-white font-medium" : "text-slate-400 hover:text-slate-200"}`}>
            {v === "markdown" ? "Markdown Doc" : "Architect View"}
          </button>
        ))}
      </nav>
      <div className="flex-1 overflow-y-auto p-3">
        {view === "architect" &&
          SECTIONS.map(({ kind, title }) => {
            const items = notes.filter((n) => n.kind === kind);
            return (
              <div key={kind} className="mb-4 rounded-lg border border-ink-700 bg-ink-900 p-3">
                <h2 className="mb-1 text-sm font-semibold text-accent">{title}</h2>
                {items.length ? <ul>{items.map((n) => <Item key={n.id} n={n} />)}</ul> : <p className="text-xs text-slate-600">None yet</p>}
              </div>
            );
          })}
        {view === "markdown" && (
          <article className="space-y-2 text-sm [&_h2]:mt-3 [&_h2]:font-semibold [&_ul]:list-disc [&_ul]:pl-5">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>{toMarkdown(notes)}</ReactMarkdown>
          </article>
        )}
      </div>
    </section>
  );
}
