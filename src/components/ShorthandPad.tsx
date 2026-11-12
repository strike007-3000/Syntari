import { useState, useRef, useEffect, useMemo } from "react";
import { useStore } from "../store";
import { fmtClock } from "../types";

export default function ShorthandPad() {
  const [text, setText] = useState("");
  const [momentTs, setMomentTs] = useState(""); // "" = end of transcript; else the chunk start (ms) a reopened-meeting note refers to
  const { notes: allNotes, addLine, transcript, running } = useStore();
  const sorted = useMemo(() => [...transcript].sort((a, b) => a.ts - b.ts), [transcript]);
  const notes = allNotes.filter((n) => !n.auto); // only what the user typed; AI suggestions live in the dashboard
  const listEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [notes.length]);

  const handleSubmit = () => {
    const trimmed = text.trim();
    if (!trimmed) return;

    // reviewing a stored meeting: the picked moment; otherwise the store stamps it
    addLine(trimmed, !running && momentTs !== "" ? Number(momentTs) : undefined);
    setText("");
    setMomentTs("");
  };

  return (
    <section className="flex h-full flex-col bg-ink-950" aria-label="Shorthand scratchpad">
      <div className="border-b border-ink-700 p-3 text-xs font-semibold uppercase tracking-wider text-slate-400">
        Shorthand Scratchpad
      </div>

      <div className="flex-1 space-y-2 overflow-y-auto p-3 font-mono text-xs">
        {notes.length === 0 ? (
          <div className="p-2 text-slate-500">Type a note and press Enter. It's filed automatically.</div>
        ) : (
          notes.map((n) => {
            const isFailed = n.status === "failed"; // failExpansion fills expanded with raw, so check failure first
            const isCompleted = !isFailed && (n.status === "completed" || !!n.expanded);
            return (
              <div key={n.id} className="flex items-start justify-between gap-2 rounded bg-ink-900/60 p-2">
                <div className="flex-1 break-words">
                  <span className="text-slate-500">[{fmtClock(n.ts)}]</span>{" "}
                  <span className="text-slate-200">{n.raw}</span>
                  {n.expanded && n.expanded !== n.raw && (
                    <div className="mt-1 text-[11px] text-slate-300 italic">
                      ↳ {n.expanded}
                    </div>
                  )}
                </div>
                <span
                  className={`whitespace-nowrap rounded px-1.5 py-0.5 text-[10px] font-medium border ${
                    isCompleted
                      ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20"
                      : isFailed
                      ? "bg-rose-500/10 text-rose-400 border-rose-500/20"
                      : "bg-amber-500/10 text-amber-400 border-amber-500/20 animate-pulse"
                  }`}
                >
                  {isCompleted
                    ? n.expanded && n.expanded !== n.raw ? "Enhanced" : "Filed"
                    : isFailed
                    ? "Failed"
                    : "Pending"}
                </span>
              </div>
            );
          })
        )}
        <div ref={listEndRef} />
      </div>

      <div className="border-t border-ink-700 bg-ink-900 p-2">
        <textarea
          id="shorthand-input"
          autoFocus
          rows={2}
          value={text}
          placeholder="Type notes as you listen…"
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              handleSubmit();
            }
          }}
          className="w-full resize-none bg-transparent p-2 font-mono text-xs text-gray-200 placeholder-slate-500 focus:outline-none"
        />
        {!running && sorted.length > 0 && (
          <select
            aria-label="Moment this note refers to"
            value={momentTs}
            onChange={(e) => setMomentTs(e.target.value)}
            className="mx-2 mb-1 w-[calc(100%-1rem)] truncate rounded bg-ink-950 p-1 text-[10px] text-slate-400"
          >
            <option value="">Refers to: end of transcript</option>
            {sorted.map((c) => (
              <option key={c.id} value={c.ts}>{fmtClock(c.ts)} [{c.speaker}] {c.text.slice(0, 40)}</option>
            ))}
          </select>
        )}
        <div className="flex justify-between px-2 pb-1 text-[10px] text-slate-500">
          <span>Press Enter to record</span>
          <span>Shift+Enter for newline</span>
        </div>
      </div>
    </section>
  );
}
