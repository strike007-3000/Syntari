import { useEffect, useState } from "react";
import { Shield, Mic, Speaker, Play, Square, AlertTriangle, ChevronLeft } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { useStore } from "../store";
import { AudioLevels, fmtClock } from "../types";
import { toMarkdown } from "./NotesDashboard";
import RecordingControls from "./RecordingControls";
import { Menu, btn, notify } from "./ui";

const Meter = ({ label, icon, v }: { label: string; icon: React.ReactNode; v: number }) => {
  const pct = Math.round(Math.min(100, v * 300));
  return (
    <div className="flex items-center gap-1.5 text-xs text-slate-400" title={`${label} level`} role="meter" aria-label={`${label} level`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
      {icon}
      <div className="h-1.5 w-14 overflow-hidden rounded bg-ink-700">
        <div className="h-full bg-accent transition-[width] duration-100" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
};

// Levels are polled 10x a second: local state here, so only the meters re-render, not the whole app.
function Meters() {
  const running = useStore((s) => s.running);
  const [levels, setLevels] = useState<AudioLevels>({ mic: 0, system: 0 });
  useEffect(() => {
    if (!running) return;
    const t = setInterval(() => invoke<AudioLevels>("get_audio_levels").then(setLevels).catch(() => {}), 100);
    return () => clearInterval(t);
  }, [running]);
  return (
    <>
      <Meter label="Mic" icon={<Mic size={14} />} v={levels.mic} />
      <Meter label="Remote" icon={<Speaker size={14} />} v={levels.system} />
    </>
  );
}

const banner = "flex w-full items-center gap-2 border-b px-4 py-1.5 text-left text-xs font-medium";

export default function Header() {
  const { running, paused, pause, resume, sessionElapsedSeconds, modelMsg, modelsReady, start, stop, tickTimer, setScreen, loaded, notes, summary, summaryStale, transcript, dbError, dismissDbError, backup } = useStore();
  const [version, setVersion] = useState("");
  useEffect(() => { getVersion().then(setVersion).catch(() => {}); }, []);
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
    <>
      <header className="flex items-center justify-between gap-4 whitespace-nowrap border-b border-ink-700 bg-ink-900 px-4 py-2">
        <div className="flex min-w-0 items-center gap-3">
          {!running && (
            <button onClick={() => setScreen("meetings")} className={`${btn.secondary} h-7 px-2`} title="Back to meetings">
              <ChevronLeft size={14} /> Meetings
            </button>
          )}
          <h1 className="text-lg font-semibold tracking-wide">Syntari</h1>
          {version && <span className="font-mono text-xs text-slate-400" title="Compare with the latest GitHub Release">v{version}</span>}
          <span title="No audio or text leaves your device" className="flex shrink-0 items-center gap-1 rounded-full bg-emerald-500/10 px-2.5 py-0.5 text-xs text-emerald-400">
            <Shield size={12} /> On-device<span className="hidden xl:inline">&nbsp;only</span>
          </span>
          <span className="hidden items-center gap-1.5 rounded-full border border-ink-700 bg-ink-800 px-2.5 py-0.5 text-xs text-slate-400 2xl:flex" title="whisper-small transcribes speech; phi-4-mini writes the notes. Both run on-device.">
            Powered by
            <span className="font-medium text-slate-100">whisper-small</span>
            <span className="text-slate-600">·</span>
            <span className="font-medium text-slate-100">phi-4-mini</span>
          </span>
        </div>
        <div className="flex shrink-0 items-center gap-4">
          <Meters />
          <RecordingControls />
          {running && (
            <span className={`flex items-center gap-1.5 text-xs font-medium ${paused ? "text-amber-400" : "text-red-400"}`}>
              <span className={`h-2 w-2 rounded-full ${paused ? "bg-amber-400" : "animate-pulse bg-red-500"}`} />
              {paused ? "Paused" : "Recording"}
            </span>
          )}
          <span role="timer" aria-label="Elapsed time" className="font-mono text-sm font-semibold tracking-wider text-slate-200">
            {fmtClock(sessionElapsedSeconds * 1000)}
          </span>
          {running && (
            <button onClick={() => (paused ? resume() : pause()).catch(notify)} className={`${btn.secondary} px-3 py-1.5 text-sm`}>
              {paused ? "Resume" : "Pause"}
            </button>
          )}
          <button
            onClick={() => (running ? stop() : start()).catch((e) => {
              const msg = String(e);
              // mic denied: offer the OS privacy page instead of a dead-end alert
              if (/microphone/i.test(msg) && confirm(`${msg}\n\nOpen the microphone privacy settings?`)) invoke("open_system_settings", { pane: "microphone" });
              else if (!/microphone/i.test(msg)) notify(msg);
            })}
            disabled={!running && !modelsReady}
            title={!running && !modelsReady ? "Waiting for AI models to load" : undefined}
            className={running ? btn.danger : `${btn.primary} shadow-lg shadow-accent/20`}
          >
            {running ? <><Square size={14} /> Stop</> : <><Play size={14} /> {loaded ? "Continue" : "Start"}</>}
          </button>
          <Menu
            label="More actions"
            disabled={running}
            items={[
              { label: "Copy Markdown", onClick: () => navigator.clipboard.writeText(toMarkdown(notes, summary, summaryStale)).then(() => notify("Copied to clipboard"), notify) },
              { label: "Export .md", title: "Save to your Downloads folder", onClick: () => invoke("export_markdown", { content: toMarkdown(notes, summary, summaryStale, transcript) }).catch(notify) },
              { label: "Back up data", title: "Save a copy of the whole meetings database to your Downloads folder", onClick: () => backup().catch((e) => notify(`Backup failed: ${e}`)) },
              { label: "Open logs", title: "Open the log folder (attach syntari.log to bug reports)", onClick: () => invoke("open_logs").catch(notify) },
            ]}
          />
        </div>
      </header>
      {modelMsg && (
        <div className={`${banner} border-amber-500/30 bg-amber-500/10 text-amber-400`} title="One-time download of the on-device models; recording unlocks when it finishes">{modelMsg}</div>
      )}
      {dbError && (
        <button type="button" onClick={dismissDbError} className={`${banner} border-red-500/40 bg-red-500/15 text-red-400`} title={`${dbError}\n\nClick to dismiss.`}>
          <AlertTriangle size={13} /> Database error: recent changes may not be saved. Export notes now. (Click to dismiss)
        </button>
      )}
      {hasSystemAudio === false && (
        <button
          type="button"
          onClick={() => invoke("open_system_settings", { pane: "screencapture" })}
          className={`${banner} border-amber-500/30 bg-amber-500/10 text-amber-400 hover:bg-amber-500/20`}
          title="Click to open macOS System Settings > Privacy & Security > Screen & System Audio Recording."
        >
          <AlertTriangle size={13} /> System audio access is required to hear the other side. Click to open Settings.
        </button>
      )}
    </>
  );
}
