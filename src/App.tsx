import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "./store";
import { NoteExpandedPayload, TranscriptChunk, AudioLevels } from "./types";
import Header from "./components/Header";
import ShorthandPad from "./components/ShorthandPad";
import NotesDashboard from "./components/NotesDashboard";
import Meetings from "./components/Meetings";
import TranscriptDrawer from "./components/TranscriptDrawer";

export default function App() {
  const { applyExpansion, pushChunk, setLevels, running, screen } = useStore();

  useEffect(() => {
    const subs = [
      listen<TranscriptChunk>("transcript-chunk", (e) => pushChunk(e.payload)),
      listen<NoteExpandedPayload>("note-expanded", (e) => applyExpansion(e.payload)),
    ];
    return () => void subs.forEach((p) => p.then((off) => off()));
  }, [applyExpansion, pushChunk]);

  useEffect(() => {
    if (!running) return;
    const t = setInterval(() => invoke<AudioLevels>("get_audio_levels").then(setLevels).catch(() => {}), 100);
    return () => clearInterval(t);
  }, [running, setLevels]);

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
