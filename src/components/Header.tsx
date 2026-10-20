import { useEffect, useState } from "react";
import { Shield, Mic, Speaker, Play, Square, AlertTriangle } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import { fmtClock } from "../types";

const Meter = ({ label, icon, v }: { label: string; icon: React.ReactNode; v: number }) => (
  <div className="flex items-center gap-2 text-xs text-slate-400" title={`${label} level`}>
    {icon}
    <div className="h-1.5 w-24 overflow-hidden rounded bg-ink-700">
      <div className="h-full bg-accent transition-[width] duration-100" style={{ width: `${Math.min(100, v * 300)}%` }} />
    </div>
  </div>
);

export default function Header() {
  const { running, sessionElapsedSeconds, levels, start, stop, tickTimer } = useStore();
  const [hasSystemAudio, setHasSystemAudio] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<{ system_audio: boolean }>("check_permissions")
      .then((res) => setHasSystemAudio(res.system_audio))
      .catch(() => setHasSystemAudio(false));
  }, []);

  useEffect(() => {
    if (!running) return;
    tickTimer();
    const t = setInterval(() => {
      tickTimer();
    }, 1000);
    return () => clearInterval(t);
  }, [running, tickTimer]);

  return (
    <header className="flex items-center justify-between border-b border-ink-700 bg-ink-900 px-4 py-2">
      <div className="flex items-center gap-4">
        <h1 className="text-lg font-semibold tracking-wide">Syntari</h1>
        <span className="flex items-center gap-1 rounded-full bg-emerald-500/10 px-2 py-0.5 text-xs text-emerald-400">
          <Shield size={12} /> 100% Air-Gapped / Zero Network Egress
        </span>
        {hasSystemAudio === false && (
          <button
            type="button"
            onClick={() => invoke("open_system_settings", { pane: "screencapture" })}
            className="flex items-center gap-1 rounded-full bg-amber-500/15 border border-amber-500/30 px-2.5 py-0.5 text-xs font-medium text-amber-400 hover:bg-amber-500/25 transition-colors cursor-pointer"
            title="Click to open macOS System Settings > Privacy & Security > Screen & System Audio Recording."
          >
            <AlertTriangle size={13} /> ⚠️ System Audio Access Required (Click to Open Settings)
          </button>
        )}
      </div>
      <div className="flex items-center gap-5">
        <Meter label="Mic" icon={<Mic size={14} />} v={levels.mic} />
        <Meter label="Remote" icon={<Speaker size={14} />} v={levels.system} />
        <span className="font-mono text-sm font-semibold tracking-wider text-slate-200">
          [{fmtClock(sessionElapsedSeconds * 1000)}]
        </span>
        <button
          onClick={() => (running ? stop() : start()).catch((e) => alert(String(e)))}
          className={`flex items-center gap-1 rounded px-3 py-1 text-sm font-medium transition-colors ${
            running ? "bg-red-500/80 hover:bg-red-500" : "bg-accent text-ink-950 hover:bg-accent/90"
          }`}
        >
          {running ? <><Square size={14} /> Stop</> : <><Play size={14} /> Start</>}
        </button>
      </div>
    </header>
  );
}
