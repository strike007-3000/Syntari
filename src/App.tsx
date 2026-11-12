import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "./store";
import { TranscriptChunk } from "./types";
import Header from "./components/Header";
import ShorthandPad from "./components/ShorthandPad";
import NotesDashboard from "./components/NotesDashboard";
import Meetings from "./components/Meetings";
import TranscriptDrawer from "./components/TranscriptDrawer";

export default function App() {
  const { pushChunk, screen } = useStore();

  useEffect(() => {
    const set = useStore.setState;
    const subs = [
      listen<TranscriptChunk>("transcript-chunk", (e) => pushChunk(e.payload)),
      listen<{ name: string; percent: number }>("model-progress", (e) =>
        set({ modelMsg: e.payload.percent < 0 ? "Verifying local models…" : `Downloading ${e.payload.name} ${Math.round(e.payload.percent)}% (one-time; Start unlocks when done)` })),
      listen("models-ready", () => set({ modelMsg: null, modelsReady: true })),
      listen<string>("model-error", (e) => set({ modelMsg: `AI unavailable: ${e.payload}` })),
    ];
    // query only after listeners are registered, so a ready event can't slip between query and listen;
    // a load failure from before this window was listening comes back as the rejection
    Promise.all(subs).then(() => invoke<boolean>("models_ready")).then((modelsReady) => set({ modelsReady })).catch((e) => set({ modelMsg: `AI unavailable: ${e}` }));
    return () => void subs.forEach((p) => p.then((off) => off()));
  }, [pushChunk]);

  return (
    <div className="flex h-full flex-col bg-ink-950">
      {screen === "meetings" ? <Meetings /> : <><Header />
      <main className="grid min-h-0 flex-1 grid-cols-[minmax(240px,1fr)_minmax(0,2fr)_auto]">
        <div className="min-h-0 border-r border-ink-700"><ShorthandPad /></div>
        <div className="min-h-0"><NotesDashboard /></div>
        <div className="min-h-0 w-80 border-l border-ink-700 [&:has(button[aria-label='Open_transcript'])]:w-auto"><TranscriptDrawer /></div>
      </main></>}
    </div>
  );
}
