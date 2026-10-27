import { useEffect, useRef, useState } from "react";
import { ChevronRight, ChevronLeft } from "lucide-react";
import { useStore } from "../store";
import { fmtClock } from "../types";

export default function TranscriptDrawer() {
  const { transcript, focusChunk } = useStore();
  const [open, setOpen] = useState(true);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (focusChunk !== null) {
      setOpen(true);
      requestAnimationFrame(() => document.getElementById(`chunk-${focusChunk}`)?.scrollIntoView({ block: "center", behavior: "smooth" }));
    } else endRef.current?.scrollIntoView();
  }, [focusChunk, transcript.length]);

  if (!open)
    return <button onClick={() => setOpen(true)} aria-label="Open transcript" className="p-2"><ChevronLeft size={16} /></button>;

  return (
    <aside className="flex h-full flex-col" aria-label="Transcript">
      <button onClick={() => setOpen(false)} className="flex items-center gap-1 border-b border-ink-700 p-3 text-xs font-semibold uppercase tracking-wider text-slate-400 hover:text-slate-200">
        <ChevronRight size={14} /> Transcript
      </button>
      <div className="flex-1 space-y-2 overflow-y-auto p-3 text-sm">
        {transcript.length === 0 && <p className="text-xs text-slate-500">Live transcript appears here once you start.</p>}
        {transcript.map((c) => (
          <p key={c.id} id={`chunk-${c.id}`} className={`rounded p-1 ${c.id === focusChunk ? "bg-accent/20" : ""}`}>
            <span className="font-mono text-xs text-slate-500">{fmtClock(c.ts)} </span>
            <b className={c.speaker === "You" ? "text-emerald-400" : "text-accent"}>[{c.speaker}]</b> {c.text}
          </p>
        ))}
        <div ref={endRef} />
      </div>
    </aside>
  );
}
