import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronRight, ChevronLeft, ArrowDown } from "lucide-react";
import { useStore } from "../store";
import { fmtClock } from "../types";

export default function TranscriptDrawer() {
  const { transcript: raw, focusChunk, toggleExcluded, running, summarizing } = useStore();
  const transcript = useMemo(() => [...raw].sort((a, b) => a.ts - b.ts), [raw]); // mic and system chunks arrive out of order
  const stick = useRef(true); // follow the live tail only while the user hasn't scrolled up
  const [away, setAway] = useState(false); // scrolled up: offer a jump back to the live tail
  const [open, setOpen] = useState(() => window.innerWidth >= 1100); // narrow windows start with the notes, transcript one click away
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (focusChunk !== null) {
      setOpen(true);
      requestAnimationFrame(() => document.getElementById(`chunk-${focusChunk}`)?.scrollIntoView({ block: "center", behavior: "smooth" }));
    } else if (stick.current) endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [focusChunk, transcript.length]);

  if (!open)
    return (
      <button onClick={() => setOpen(true)} aria-label="Open transcript" className="flex h-full flex-col items-center gap-2 px-2 py-3 text-xs font-semibold uppercase tracking-wider text-slate-400 hover:text-slate-200">
        <ChevronLeft size={16} />
        <span className="[writing-mode:vertical-rl]">Transcript</span>
        {running && <span className="h-2 w-2 animate-pulse rounded-full bg-red-500" title="Live" />}
      </button>
    );

  return (
    <aside className="relative flex h-full flex-col" aria-label="Transcript">
      <button onClick={() => setOpen(false)} aria-label="Collapse transcript" className="flex h-11 shrink-0 items-center gap-1 border-b border-ink-700 px-3 text-xs font-semibold uppercase tracking-wider text-slate-400 hover:text-slate-200">
        <ChevronRight size={14} /> Transcript
      </button>
      <div
        onScroll={(e) => { const el = e.currentTarget; stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 80; setAway(!stick.current); }}
        aria-live={running ? "polite" : "off"}
        className="flex-1 space-y-2 overflow-y-auto p-3 text-sm"
      >
        {transcript.length === 0 && <p className="text-xs text-slate-400">Live transcript appears here once you start.</p>}
        {transcript.map((c) => (
          <p key={c.id} id={`chunk-${c.id}`} className={`group relative rounded p-1 ${c.id === focusChunk ? "bg-accent/20" : ""}`}>
            {/* only the text is dimmed, so the "include" button on an excluded line stays readable */}
            <span className={c.excluded ? "opacity-40 line-through" : ""}>
              <span className="mr-1.5 font-mono text-xs text-slate-400">{fmtClock(c.ts)}</span>
              <b className={`mr-1 font-semibold ${c.speaker === "You" ? "text-emerald-400" : "text-accent"}`}>{c.speaker}</b> {c.text}
            </span>
            {!running && !summarizing && (
              // absolutely positioned so the hidden button never wraps onto its own line and adds blank height
              <button onClick={() => toggleExcluded(c.id)} title={c.excluded ? "Include in summary again" : "Exclude from summary (e.g. an ad)"} className="absolute right-1 top-1 rounded bg-ink-800 px-1.5 py-0.5 text-xs text-slate-400 opacity-0 shadow hover:text-slate-200 focus:opacity-100 group-hover:opacity-100">
                {c.excluded ? "↩ include" : "✕ exclude"}
              </button>
            )}
          </p>
        ))}
        <div ref={endRef} />
      </div>
      {away && transcript.length > 0 && (
        <button
          onClick={() => { stick.current = true; setAway(false); endRef.current?.scrollIntoView({ behavior: "smooth" }); }}
          className="absolute bottom-3 left-1/2 flex -translate-x-1/2 items-center gap-1 rounded-full bg-accent px-3 py-1 text-xs font-semibold text-ink-950 shadow-lg"
        >
          <ArrowDown size={12} /> {running ? "Jump to live" : "Jump to end"}
        </button>
      )}
    </aside>
  );
}
