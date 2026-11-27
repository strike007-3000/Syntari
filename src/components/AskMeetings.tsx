import { useRef, useState } from "react";
import { Search, Sparkles, X } from "lucide-react";
import { useStore } from "../store";
import { Source } from "../ask";
import { fmtClock } from "../types";
import { btn, notify } from "./ui";

type Mode = "search" | "ask";

/**
 * One bar for both ways of finding things: Search filters the meeting list as you type;
 * Ask answers a question across all meetings, citing numbered sources that open the meeting at that moment.
 */
export default function AskMeetings({ onSearch }: { onSearch: (q: string) => void }) {
  const { ask, openSource, modelsReady, running } = useStore();
  const [mode, setMode] = useState<Mode>("search");
  const [q, setQ] = useState("");
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [res, setRes] = useState<{ question: string; answer: string; sources: Source[] } | null>(null);
  const askBlocked = running ? "Stop the session first" : !modelsReady ? "Waiting for AI models to load" : "";

  const go = () => {
    if (!q.trim() || busy || askBlocked) return;
    setBusy(true);
    const question = q.trim();
    ask(question).then((r) => setRes({ question, ...r })).catch((e) => setRes({ question, answer: `Could not answer: ${e}`, sources: [] })).finally(() => setBusy(false));
  };
  const switchTo = (m: Mode) => {
    setMode(m);
    onSearch(m === "search" ? q : ""); // a question typed in Ask mode must not filter the list
    input.current?.focus();
  };
  const change = (v: string) => {
    setQ(v);
    if (mode === "search") onSearch(v);
  };
  const open = (s: Source) => openSource(s).catch(notify);
  const tab = (m: Mode, icon: React.ReactNode, label: string) => (
    <button
      type="button"
      aria-pressed={mode === m}
      onClick={() => switchTo(m)}
      className={`flex items-center gap-1.5 rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${mode === m ? "bg-ink-700 text-white" : "text-slate-400 hover:text-slate-200"}`}
    >
      {icon} {label}
    </button>
  );

  return (
    <section className="mb-6" aria-label="Search or ask across meetings">
      <div className="flex items-center gap-2 rounded-xl border border-ink-700 bg-ink-900 p-1.5 focus-within:border-accent/60">
        <div className="flex shrink-0 gap-0.5 rounded-lg bg-ink-950 p-0.5" role="group" aria-label="Mode">
          {tab("search", <Search size={13} />, "Search")}
          {tab("ask", <Sparkles size={13} />, "Ask AI")}
        </div>
        <input
          ref={input}
          value={q}
          onChange={(e) => change(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter" && mode === "ask") go(); if (e.key === "Escape") change(""); }}
          aria-label={mode === "search" ? "Search meetings" : "Ask a question across meetings"}
          placeholder={mode === "search" ? "Search titles, summaries, notes and transcripts" : "Ask a question, e.g. what did we decide about pricing?"}
          className="min-w-0 flex-1 bg-transparent px-2 py-1.5 text-sm placeholder-slate-400 outline-none"
        />
        {q && (
          <button type="button" aria-label="Clear" onClick={() => change("")} className="rounded p-1 text-slate-400 hover:text-white"><X size={14} /></button>
        )}
        {mode === "ask" && (
          <button onClick={go} disabled={busy || !!askBlocked || !q.trim()} title={askBlocked || undefined} className={btn.primary}>
            {busy ? "Thinking…" : "Ask"}
          </button>
        )}
      </div>
      {mode === "ask" && askBlocked && <p className="mt-2 px-1 text-xs text-amber-400">{askBlocked}.</p>}
      {mode === "ask" && !res && !askBlocked && (
        <p className="mt-2 px-1 text-xs text-slate-400">Answers come from your meetings, on this device, with numbered links to the exact moment.</p>
      )}
      {res && (
        <div className="mt-3 rounded-xl border border-ink-700 bg-ink-900 p-4 text-sm">
          <div className="mb-2 flex items-start justify-between gap-3">
            <p className="flex items-center gap-1.5 text-xs font-medium text-slate-400"><Sparkles size={12} className="text-accent" /> {res.question}</p>
            <button aria-label="Close answer" onClick={() => setRes(null)} className="rounded p-0.5 text-slate-400 hover:text-white"><X size={14} /></button>
          </div>
          <p className="whitespace-pre-wrap leading-relaxed text-slate-100">
            {res.answer.split(/(\[\d+\])/).map((part, i) => {
              const s = /^\[(\d+)\]$/.test(part) ? res.sources.find((x) => x.n === Number(part.slice(1, -1))) : undefined;
              return s ? <button key={i} onClick={() => open(s)} className="mx-0.5 rounded bg-accent/20 px-1.5 py-0.5 text-xs font-semibold text-accent hover:bg-accent/40" title="Open this moment">{part}</button> : part;
            })}
          </p>
          {res.sources.length > 0 && (
            <>
              <h3 className="mb-1 mt-4 text-xs font-semibold uppercase tracking-wider text-slate-400">Sources</h3>
              <ul className="space-y-0.5">
                {res.sources.map((s) => (
                  <li key={s.n}>
                    <button onClick={() => open(s)} className="flex w-full gap-2 rounded px-2 py-1 text-left text-sm text-slate-300 hover:bg-ink-800 hover:text-white">
                      <span className="shrink-0 font-mono text-xs leading-5 text-accent">[{s.n}]</span>
                      <span className="min-w-0">
                        <span className="font-medium text-slate-200">{s.title || new Date(s.startedAt).toLocaleDateString()}</span>
                        <span className="text-slate-400"> · {s.summary ? "summary" : fmtClock(s.ts)}{s.speaker ? ` · ${s.speaker}` : ""}</span>
                        <span className="block truncate text-slate-400">{s.text}</span>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
    </section>
  );
}
