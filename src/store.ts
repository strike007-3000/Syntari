import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import Database from "@tauri-apps/plugin-sql";
import { Note, NoteExpandedPayload, TranscriptChunk, AudioLevels, parseShorthand } from "./types";

export type View = "architect" | "markdown";

interface InsightItem {
  kind: string;
  text: string;
  owner?: string;
  due?: string;
}

interface State {
  running: boolean;
  sessionId: number | null;
  startedAt: number;
  sessionElapsedSeconds: number;
  notes: Note[];
  transcript: TranscriptChunk[];
  levels: AudioLevels;
  view: View;
  focusChunk: number | null;
  setView: (v: View) => void;
  tickTimer: () => void;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  addLine: (line: string, explicitTs?: number) => Promise<void>;
  toggleDone: (id: string) => void;
  applyExpansion: (e: NoteExpandedPayload) => void;
  failExpansion: (id: string) => void;
  pushChunk: (c: TranscriptChunk) => void;
  setLevels: (l: AudioLevels) => void;
  inspect: (chunkId: number | null) => void;
  pushAutoNote: (item: InsightItem, tsMs: number) => void;
  acceptSuggestion: (id: string) => void;
  dismissSuggestion: (id: string) => void;
}

let db: Promise<Database> | null = null;
let insightInterval: ReturnType<typeof setInterval> | null = null;
// Tracks session-relative ms of last extracted insight to avoid re-processing.
let lastInsightMs = 0;
// Dedup set so repeated extractions don't push duplicate notes.
const seenInsightTexts = new Set<string>();
const getDb = () => {
  if (!db) {
    db = invoke<string>("get_db_key").then((key) => {
      const url = key ? `sqlite:syntari.db?key=${key}` : "sqlite:syntari.db";
      return Database.load(url);
    });
  }
  return db;
};
const persist = (fn: (d: Database) => Promise<unknown>) => getDb().then(fn).catch((e) => console.error("db", e));

export const useStore = create<State>((set, get) => ({
  running: false,
  sessionId: null,
  startedAt: 0,
  sessionElapsedSeconds: 0,
  notes: [],
  transcript: [],
  levels: { mic: 0, system: 0 },
  view: "architect",
  focusChunk: null,
  setView: (view) => set({ view }),

  tickTimer: () => {
    const { running, startedAt } = get();
    if (running && startedAt > 0) {
      set({ sessionElapsedSeconds: Math.floor((Date.now() - startedAt) / 1000) });
    }
  },

  async start() {
    await invoke("start_syntari_session");
    const startedAt = Date.now();
    lastInsightMs = 0;
    seenInsightTexts.clear();
    set({
      running: true,
      startedAt,
      sessionElapsedSeconds: 0,
      notes: [],
      transcript: [],
      sessionId: null,
    });
    persist(async (d) => {
      const r = await d.execute("INSERT INTO sessions (started_at) VALUES (?)", [startedAt]);
      set({ sessionId: r.lastInsertId ?? null });
    });
    // Poll for insights every 30s
    insightInterval = setInterval(async () => {
      const { running, startedAt: t0 } = get();
      if (!running) return;
      const nowMs = Date.now() - t0;
      try {
        const items = await invoke<InsightItem[]>("extract_insights", { sinceMs: lastInsightMs });
        lastInsightMs = Math.max(0, nowMs - 5000); // leave 5s overlap for in-flight chunks
        for (const item of items) {
          get().pushAutoNote(item, nowMs);
        }
      } catch (e) {
        console.error("extract_insights failed:", e);
      }
    }, 30_000);
  },

  async stop() {
    if (insightInterval) { clearInterval(insightInterval); insightInterval = null; }
    await invoke("stop_syntari_session");
    const { sessionId } = get();
    set({ running: false });
    if (sessionId) persist((d) => d.execute("UPDATE sessions SET ended_at=? WHERE id=?", [Date.now(), sessionId]));
  },

  async addLine(line, explicitTs) {
    const { running, startedAt, sessionId, transcript, sessionElapsedSeconds } = get();
    if (!line.trim()) return;

    let ts = explicitTs;
    if (ts === undefined) {
      if (transcript.length > 0) {
        const lastChunk = transcript[transcript.length - 1];
        ts = lastChunk.end !== undefined ? lastChunk.end * 1000 : lastChunk.ts;
      } else if (running && startedAt > 0) {
        ts = Math.max(0, Date.now() - startedAt);
      } else if (sessionElapsedSeconds > 0) {
        ts = sessionElapsedSeconds * 1000;
      } else {
        ts = 0;
      }
    }

    const p = parseShorthand(line);
    const note: Note = {
      id: crypto.randomUUID(),
      kind: p.kind,
      raw: p.text,
      ts,
      status: running ? "pending" : "completed",
      owner: p.owner,
      due: p.due,
    };

    set((s) => ({ notes: [...s.notes, note] }));

    persist((d) =>
      d.execute(
        "INSERT INTO notes (id, session_id, kind, raw, ts_ms, owner, due) VALUES (?,?,?,?,?,?,?)",
        [note.id, sessionId, note.kind, note.raw, ts, note.owner ?? null, note.due ?? null],
      ),
    );

    if (running) {
      invoke<NoteExpandedPayload>("expand_note", {
        id: note.id,
        kind: note.kind,
        text: note.raw,
        tsMs: ts,
      })
        .then((res) => {
          if (res) get().applyExpansion({ ...res, id: note.id });
        })
        .catch((err) => {
          console.error("expand_note failed:", err);
          get().failExpansion(note.id);
        });
    }
  },

  toggleDone(id) {
    set((s) => ({ notes: s.notes.map((n) => (n.id === id ? { ...n, done: !n.done } : n)) }));
    const n = get().notes.find((x) => x.id === id);
    if (n) persist((d) => d.execute("UPDATE notes SET done=? WHERE id=?", [n.done ? 1 : 0, id]));
  },

  applyExpansion(e) {
    set((s) => ({
      notes: s.notes.map((n) =>
        n.id === e.id
          ? {
              ...n,
              status: "completed",
              expanded: e.enhanced_text || n.raw,
              citation: e.citation_quote
                ? {
                    chunkIds: [],
                    quote: e.citation_quote,
                    start: e.citation_start,
                    end: e.citation_end,
                  }
                : undefined,
            }
          : n,
      ),
    }));

    persist((d) =>
      d.execute(
        "UPDATE notes SET expanded=?, citation_quote=?, citation_chunks=? WHERE id=?",
        [e.enhanced_text, e.citation_quote ?? null, "[]", e.id],
      ),
    );
  },

  failExpansion(id) {
    set((s) => ({
      notes: s.notes.map((n) =>
        n.id === id ? { ...n, status: "failed", expanded: n.raw } : n,
      ),
    }));
  },

  pushChunk(c) {
    set((s) => ({ transcript: [...s.transcript, c] }));
    const { sessionId } = get();
    persist((d) =>
      d.execute(
        "INSERT INTO transcript (session_id, chunk_id, speaker, text, ts_ms) VALUES (?,?,?,?,?)",
        [sessionId, c.id, c.speaker, c.text, c.ts],
      ),
    );
  },

  pushAutoNote(item, tsMs) {
    const key = `${item.kind}:${item.text.toLowerCase().trim()}`;
    if (seenInsightTexts.has(key)) return;
    seenInsightTexts.add(key);
    const { sessionId } = get();
    const note: Note = {
      id: crypto.randomUUID(),
      kind: item.kind as Note["kind"],
      raw: item.text,
      ts: tsMs,
      status: "suggested",
      owner: item.owner,
      due: item.due,
    };
    set((s) => ({ notes: [...s.notes, note] }));
  },

  acceptSuggestion(id) {
    const note = get().notes.find((n) => n.id === id);
    if (!note) return;
    set((s) => ({ notes: s.notes.map((n) => n.id === id ? { ...n, status: "completed", expanded: n.raw } : n) }));
    const { sessionId } = get();
    persist((d) =>
      d.execute(
        "INSERT INTO notes (id, session_id, kind, raw, ts_ms, owner, due, expanded) VALUES (?,?,?,?,?,?,?,?)",
        [note.id, sessionId, note.kind, note.raw, note.ts, note.owner ?? null, note.due ?? null, note.raw],
      ),
    );
  },

  dismissSuggestion(id) {
    set((s) => ({ notes: s.notes.filter((n) => n.id !== id) }));
  },

  setLevels: (levels) => set({ levels }),
  inspect: (focusChunk) => set({ focusChunk }),
}));
