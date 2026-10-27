import { useEffect, useState } from "react";
import { Shield, Mic, Speaker, Play, Square, AlertTriangle } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useStore } from "../store";
import { fmtClock } from "../types";
import { toMarkdown } from "./NotesDashboard";

const Meter = ({ label, icon, v }: { label: string; icon: React.ReactNode; v: number }) => (
  <div className="flex items-center gap-1.5 text-xs text-slate-400" title={`${label} level`}>
    {icon}
    <div className="h-1.5 w-14 overflow-hidden rounded bg-ink-700">
      <div className="h-full bg-accent transition-[width] duration-100" style={{ width: `${Math.min(100, v * 300)}%` }} />
    </div>
  </div>
);

export default function Header() {
  const { running, sessionElapsedSeconds, levels, start, stop, tickTimer, setScreen, loaded, notes, summary } = useStore();
  const [hasSystemAudio, setHasSystemAudio] = useState<boolean | null>(null);

  // null = no status line; string = status line for the one-time model download or an init error
  const [modelMsg, setModelMsg] = useState<string | null>(null);
  // Start stays disabled until the backend reports the models loaded
  const [modelsReady, setModelsReady] = useState(false);
  useEffect(() => {
    const subs = [
      listen<{ name: string; percent: number }>("model-progress", (e) =>
        setModelMsg(`Downloading ${e.payload.name} (one-time) ${Math.round(e.payload.percent)}%`)),
      listen("models-ready", () => { setModelMsg(null); setModelsReady(true); }),
      listen<string>("model-error", (e) => setModelMsg(`AI unavailable: ${e.payload}`)),
    ];
    // query only after listeners are registered, so a ready event can't slip between query and listen
    Promise.all(subs).then(() => invoke<boolean>("models_ready")).then(setModelsReady).catch(() => {});
    return () => void subs.forEach((p) => p.then((off) => off()));
  }, []);

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
    <header className="flex items-center justify-between gap-4 whitespace-nowrap border-b border-ink-700 bg-ink-900 px-4 py-2">
      <div className="flex min-w-0 items-center gap-3">
        <h1 className="text-lg font-semibold tracking-wide">Syntari</h1>
        <span title="No audio or text leaves your device" className="flex items-center gap-1 rounded-full bg-emerald-500/10 px-2.5 py-0.5 text-xs text-emerald-400">
          <Shield size={12} /> On-device only
        </span>
        <span className="flex items-center gap-1.5 rounded-full border border-ink-700 bg-ink-800 px-2.5 py-0.5 text-xs text-slate-400" title="whisper-small transcribes speech; phi-4-mini writes the notes. Both run on-device.">
          Powered by
          <span className="font-medium text-slate-100">whisper-small</span>
          <span className="text-slate-600">·</span>
          <span className="font-medium text-slate-100">phi-4-mini</span>
        </span>
        {modelMsg && (
          <span className="rounded-full border border-amber-500/30 bg-amber-500/15 px-2.5 py-0.5 text-xs text-amber-400">{modelMsg}</span>
        )}
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
      <div className="flex shrink-0 items-center gap-4">
        <Meter label="Mic" icon={<Mic size={14} />} v={levels.mic} />
        <Meter label="Remote" icon={<Speaker size={14} />} v={levels.system} />
        {!running && (
          <>
            <button
              onClick={() => setScreen("meetings")}
              className="rounded-md border border-ink-700 bg-ink-800 px-2.5 py-1 text-xs text-slate-300 transition-colors hover:text-white"
            >
              ← Meetings
            </button>
            <button
              onClick={() => navigator.clipboard.writeText(toMarkdown(notes, summary))}
              className="rounded-md border border-ink-700 bg-ink-800 px-2.5 py-1 text-xs text-slate-300 transition-colors hover:text-white"
            >
              Copy Markdown
            </button>
            <button
              onClick={() => invoke("export_markdown", { content: toMarkdown(notes, summary) }).catch((e) => alert(String(e)))}
              className="rounded-md border border-ink-700 bg-ink-800 px-2.5 py-1 text-xs text-slate-300 transition-colors hover:text-white"
              title="Save to your Downloads folder"
            >
              Export .md
            </button>
          </>
        )}
        <span className="font-mono text-sm font-semibold tracking-wider text-slate-200">
          [{fmtClock(sessionElapsedSeconds * 1000)}]
        </span>
        <button
          onClick={() => (running ? stop() : start()).catch((e) => alert(String(e)))}
          disabled={!running && !modelsReady}
          title={!running && !modelsReady ? "Waiting for AI models to load" : undefined}
          className={`flex items-center gap-1.5 rounded-md px-4 py-1.5 text-sm font-semibold transition-colors disabled:opacity-50 disabled:cursor-not-allowed ${
            running ? "bg-red-500/80 hover:bg-red-500" : "bg-accent text-ink-950 shadow-lg shadow-accent/20 hover:bg-accent/90"
          }`}
        >
          {running ? <><Square size={14} /> Stop</> : <><Play size={14} /> {loaded ? "Continue" : "Start"}</>}
        </button>
      </div>
    </header>
  );
}
