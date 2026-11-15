import { useState } from "react";
import { useStore } from "../store";
import { Source } from "../ask";
import { fmtClock } from "../types";

/** Ask a question across all meetings; the answer cites numbered sources that open the meeting at that moment. */
export default function AskMeetings() {
  const { ask, openSource, modelsReady, running } = useStore();
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState(false);
  const [res, setRes] = useState<{ answer: string; sources: Source[] } | null>(null);
  const go = () => {
    if (!q.trim() || busy) return;
    setBusy(true);
    ask(q).then(setRes).catch((e) => setRes({ answer: `Could not answer: ${e}`, sources: [] })).finally(() => setBusy(false));
  };
  const open = (s: Source) => openSource(s).catch((e) => alert(String(e)));
  return (
    <section className="mb-6 rounded-lg border border-ink-700 bg-ink-900 p-4" aria-label="Ask across meetings">
      <div className="flex gap-2">
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && go()}
          placeholder="Ask across all your meetings, e.g. what did we decide about pricing?"
          className="min-w-0 flex-1 rounded-lg border border-ink-700 bg-ink-950 px-3 py-2 text-sm placeholder-slate-500 focus:outline-none"
        />
        <button onClick={go} disabled={busy || !modelsReady || running || !q.trim()} title={running ? "Stop the session first" : !modelsReady ? "Waiting for AI models to load" : undefined} className="rounded-md bg-accent px-4 py-1.5 text-sm font-semibold text-ink-950 disabled:opacity-50">
          {busy ? "Thinking…" : "Ask"}
        </button>
      </div>
      {res && (
        <div className="mt-3 space-y-2 text-sm">
          <p className="whitespace-pre-wrap">
            {res.answer.split(/(\[\d+\])/).map((part, i) => {
              const s = /^\[(\d+)\]$/.test(part) ? res.sources.find((x) => x.n === Number(part.slice(1, -1))) : undefined;
              return s ? <button key={i} onClick={() => open(s)} className="mx-0.5 rounded bg-accent/20 px-1 text-xs text-accent hover:bg-accent/40" title="Open this moment">{part}</button> : part;
            })}
          </p>
          {res.sources.length > 0 && (
            <ul className="space-y-1 border-t border-ink-700 pt-2 text-xs text-slate-400">
              {res.sources.map((s) => (
                <li key={s.n}>
                  <button onClick={() => open(s)} className="text-left hover:text-slate-200">
                    <span className="font-mono text-accent">[{s.n}]</span> {s.title || new Date(s.startedAt).toLocaleDateString()} · {fmtClock(s.ts)}{s.speaker ? ` · ${s.speaker}` : ""}: {s.text.slice(0, 140)}{s.text.length > 140 ? "…" : ""}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </section>
  );
}
