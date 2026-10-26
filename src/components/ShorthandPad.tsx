import { useState, useRef, useEffect } from "react";
import { useStore } from "../store";
import { fmtClock } from "../types";

export default function ShorthandPad() {
  const [text, setText] = useState("");
  const { notes: allNotes, addLine, transcript, sessionElapsedSeconds } = useStore();
  const notes = allNotes.filter((n) => n.status !== "suggested"); // AI suggestions live in the dashboard only
  const listEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [notes.length]);

  const handleSubmit = () => {
    const trimmed = text.trim();
    if (!trimmed) return;

    // Pull accurate audio timestamp
    let ts = 0;
    if (transcript.length > 0) {
      const last = transcript[transcript.length - 1];
      ts = last.end !== undefined ? last.end * 1000 : last.ts;
    } else if (sessionElapsedSeconds > 0) {
      ts = sessionElapsedSeconds * 1000;
    }

    addLine(trimmed, ts);
    setText("");
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
            const isCompleted = n.status === "completed" || !!n.expanded;
            const isFailed = n.status === "failed";
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
                    ? "Enhanced"
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
        <div className="flex justify-between px-2 pb-1 text-[10px] text-slate-500">
          <span>Press Enter to record</span>
          <span>Shift+Enter for newline</span>
        </div>
      </div>
    </section>
  );
}
