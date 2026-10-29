import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import Database from "@tauri-apps/plugin-sql";
import { Note, NoteExpandedPayload, TranscriptChunk, AudioLevels, parseShorthand } from "./types";

export type View = "notes" | "summary" | "markdown";

interface InsightItem {
  kind: string;
  text: string;
  owner?: string;
  due?: string;
}

export interface MeetingRow { id: number; started_at: number; summary: string | null; title: string | null; first_note: string | null }

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
  screen: "meetings" | "session";
  loaded: boolean; // current session came from history/stop, so Start continues it
  durationMs: number;
  setScreen: (s: "meetings" | "session") => void;
  newMeeting: () => void;
  summary: string;
  summarizing: boolean;
  summarize: () => Promise<void>;
  setView: (v: View) => void;
  tickTimer: () => void;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  addLine: (line: string, explicitTs?: number) => Promise<void>;
  toggleDone: (id: string) => void;
  setKind: (id: string, kind: Note["kind"]) => void;
  applyExpansion: (e: NoteExpandedPayload) => void;
  failExpansion: (id: string) => void;
  pushChunk: (c: TranscriptChunk) => void;
  setLevels: (l: AudioLevels) => void;
  inspect: (chunkId: number | null) => void;
  pushAutoNote: (item: InsightItem, tsMs: number) => void;
  acceptSuggestion: (id: string) => void;
  dismissSuggestion: (id: string) => void;
  listSessions: (q?: string) => Promise<MeetingRow[]>;
  loadSession: (id: number) => Promise<void>;
  dbError: string | null; // set when a write fails, so the UI can warn instead of silently losing data
  dismissDbError: () => void;
  deleteSession: (id: number) => Promise<void>;
  deleteNote: (id: string) => void;
  editNote: (id: string, text: string) => void;
  renameSession: (id: number, title: string) => Promise<void>;
  backup: () => Promise<string>;
}

let db: Promise<Database> | null = null;
let insightInterval: ReturnType<typeof setInterval> | null = null;
// Tracks session-relative ms of last extracted insight to avoid re-processing.
let lastInsightMs = 0;
// Dedup set so repeated extractions don't push duplicate notes.
const seenInsightTexts = new Set<string>();
// Notes whose category the user typed (dec:/act:/risk:); the model must not re-bucket these.
const typedKinds = new Set<string>();
const editedNotes = new Set<string>(); // manual text edits must survive a late model expansion
const getDb = () => {
  if (!db) {
    db = Database.load("sqlite:syntari.db");
  }
  return db;
};
const persist = (fn: (d: Database) => Promise<unknown>) =>
  getDb().then(fn).catch((e) => {
    console.error("db", e);
    db = null; // allow a retry if opening the database was what failed
    useStore.setState({ dbError: String(e) });
  });

export const useStore = create<State>((set, get) => ({
  running: false,
  sessionId: null,
  startedAt: 0,
  sessionElapsedSeconds: 0,
  notes: [],
  transcript: [],
  levels: { mic: 0, system: 0 },
  view: "notes",
  focusChunk: null,
  screen: "meetings",
  loaded: false,
  durationMs: 0,
  setScreen: (screen) => set({ screen }),
  newMeeting: () => {
    if (get().running) return;
    set({ screen: "session", sessionId: null, loaded: false, durationMs: 0, sessionElapsedSeconds: 0, notes: [], transcript: [], summary: "", view: "notes" });
  },
  summary: "",
  summarizing: false,
  setView: (view) => set({ view }),

  tickTimer: () => {
    const { running, startedAt } = get();
    if (running && startedAt > 0) {
      set({ sessionElapsedSeconds: Math.floor((Date.now() - startedAt) / 1000) });
    }
  },

  async start() {
    // A loaded past session is continued in place; otherwise a new one is created.
    const { loaded, sessionId: prevId, durationMs, notes: prevNotes } = get();
    const resume = loaded && prevId !== null;
    const offset = resume ? durationMs : 0;
    const startedAt = Date.now() - offset;
    // Create the DB session first so no note/chunk can be saved with a null sessionId
    // Best-effort: capture must still work when the keychain/SQL plugin is unavailable (sessionId stays null)
    let db: Database | null = null;
    let sessionId: number | null = resume ? prevId : null;
    if (!resume) {
      try {
        db = await getDb();
        sessionId = (await db.execute("INSERT INTO sessions (started_at) VALUES (?)", [startedAt])).lastInsertId ?? null;
      } catch (e) {
        console.error("db", e);
        set({ dbError: String(e) });
      }
    }
    try {
      // chunks carry only their start time; reuse it as both bounds for the lookback window
      const prior = resume ? get().transcript.map((c) => ({ start: c.ts / 1000, end: c.ts / 1000, speaker: c.speaker, text: c.text })) : [];
      await invoke("start_syntari_session", { offsetMs: offset, prior });
    } catch (e) {
      if (db && !resume && sessionId !== null) await db.execute("DELETE FROM sessions WHERE id=?", [sessionId]).catch(() => {});
      throw e;
    }
    lastInsightMs = offset;
    seenInsightTexts.clear();
    if (resume) prevNotes.forEach((n) => seenInsightTexts.add(`${n.kind}:${(n.expanded ?? n.raw).toLowerCase().trim()}`));
    set({
      running: true,
      startedAt,
      sessionElapsedSeconds: Math.floor(offset / 1000),
      sessionId,
      loaded: sessionId !== null,
      ...(resume ? {} : { notes: [], transcript: [], summary: "" }),
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
    const { sessionId, startedAt } = get();
    set({ running: false, durationMs: Date.now() - startedAt });
    if (sessionId) persist((d) => d.execute("UPDATE sessions SET ended_at=? WHERE id=?", [Date.now(), sessionId]));
  },

  async summarize() {
    const { transcript, sessionId } = get();
    if (!transcript.length) throw new Error("No transcript to summarize");
    set({ summarizing: true });
    try {
      const chunks = [...transcript]
        .sort((a, b) => a.ts - b.ts)
        .map((c) => ({ start: c.start ?? c.ts / 1000, end: c.end ?? c.ts / 1000, speaker: c.speaker, text: c.text }));
      const summary = await invoke<string>("summarize_transcript", { chunks });
      set({ summary, view: "summary" });
      if (sessionId) persist((d) => d.execute("UPDATE sessions SET summary=? WHERE id=?", [summary, sessionId]));
    } finally {
      set({ summarizing: false });
    }
  },

  async addLine(line, explicitTs) {
    const { running, startedAt, sessionId, transcript, sessionElapsedSeconds } = get();
    if (!line.trim()) return;

    let ts = explicitTs;
    if (ts === undefined) {
      if (transcript.length > 0) {
        // chunk ids are random, so never trust array order for "latest"
        ts = Math.max(...transcript.map((c) => (c.end !== undefined ? c.end * 1000 : c.ts)));
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
      status: "pending",
      owner: p.owner,
      due: p.due,
    };

    if (/^(dec|act|risk):/i.test(line.trim())) typedKinds.add(note.id);
    set((s) => ({ notes: [...s.notes, note] }));

    persist((d) =>
      d.execute(
        "INSERT INTO notes (id, session_id, kind, raw, ts_ms, owner, due) VALUES (?,?,?,?,?,?,?)",
        [note.id, sessionId, note.kind, note.raw, ts, note.owner ?? null, note.due ?? null],
      ),
    );

    invoke<NoteExpandedPayload>("expand_note", {
      id: note.id,
      kind: note.kind,
      text: note.raw,
      tsMs: ts,
      // reviewing a stored meeting: Rust holds no transcript, so send it (ignored while recording)
      context: running ? null : transcript.map((c) => ({ start: c.ts / 1000, end: c.ts / 1000, speaker: c.speaker, text: c.text })),
    })
      .then((res) => {
        if (res) get().applyExpansion({ ...res, id: note.id });
      })
      .catch((err) => {
        console.error("expand_note failed:", err);
        get().failExpansion(note.id);
      });
  },

  setKind(id, kind) {
    typedKinds.add(id); // a manual choice beats a late model answer
    set((s) => ({ notes: s.notes.map((n) => (n.id === id ? { ...n, kind } : n)) }));
    persist((d) => d.execute("UPDATE notes SET kind=? WHERE id=?", [kind, id]));
  },

  toggleDone(id) {
    set((s) => ({ notes: s.notes.map((n) => (n.id === id ? { ...n, done: !n.done } : n)) }));
    const n = get().notes.find((x) => x.id === id);
    if (n) persist((d) => d.execute("UPDATE notes SET done=? WHERE id=?", [n.done ? 1 : 0, id]));
  },

  applyExpansion(e) {
    if (!get().notes.some((n) => n.id === e.id)) return; // note was deleted while the model ran
    set((s) => ({
      notes: s.notes.map((n) =>
        n.id === e.id
          ? {
              ...n,
              kind: e.category && !typedKinds.has(n.id) ? e.category : n.kind,
              owner: n.owner ?? e.owner ?? undefined,
              due: n.due ?? e.due ?? undefined,
              status: "completed",
              expanded: editedNotes.has(n.id) ? n.expanded : e.enhanced_text || n.raw,
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
        "UPDATE notes SET expanded=COALESCE(?, expanded), citation_quote=?, citation_start=?, citation_chunks=?, kind=COALESCE(?, kind), owner=COALESCE(owner, ?), due=COALESCE(due, ?) WHERE id=?",
        [editedNotes.has(e.id) ? null : e.enhanced_text, e.citation_quote ?? null, e.citation_start ?? null, "[]", typedKinds.has(e.id) ? null : e.category ?? null, e.owner ?? null, e.due ?? null, e.id],
      ),
    );
  },

  failExpansion(id) {
    set((s) => ({
      notes: s.notes.map((n) =>
        n.id === id ? { ...n, status: "failed", expanded: editedNotes.has(id) ? n.expanded : n.raw } : n,
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
      auto: true,
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
        "INSERT INTO notes (id, session_id, kind, raw, ts_ms, owner, due, expanded, auto) VALUES (?,?,?,?,?,?,?,?,1)",
        [note.id, sessionId, note.kind, note.raw, note.ts, note.owner ?? null, note.due ?? null, note.raw],
      ),
    );
  },

  dismissSuggestion(id) {
    set((s) => ({ notes: s.notes.filter((n) => n.id !== id) }));
  },

  async listSessions(q = "") {
    const like = `%${q.trim()}%`;
    return (await getDb()).select(
      `SELECT id, started_at, summary, title,
         (SELECT COALESCE(expanded, raw) FROM notes n WHERE n.session_id = s.id ORDER BY ts_ms LIMIT 1) AS first_note
       FROM sessions s WHERE ?1 = '%%' OR summary LIKE ?1 OR title LIKE ?1
         OR EXISTS (SELECT 1 FROM notes n WHERE n.session_id = s.id AND (n.raw LIKE ?1 OR n.expanded LIKE ?1))
         OR EXISTS (SELECT 1 FROM transcript t WHERE t.session_id = s.id AND t.text LIKE ?1)
       ORDER BY started_at DESC LIMIT 200`,
      [like],
    );
  },

  async loadSession(id) {
    if (get().running) return;
    const d = await getDb();
    const rows = await d.select<any[]>("SELECT * FROM notes WHERE session_id=? ORDER BY ts_ms", [id]);
    const chunks = await d.select<any[]>("SELECT * FROM transcript WHERE session_id=? ORDER BY ts_ms, rowid", [id]);
    const sum = await d.select<{ summary: string | null }[]>("SELECT summary FROM sessions WHERE id=?", [id]);
    const durationMs = Math.max(0, ...chunks.map((r) => r.ts_ms), ...rows.map((r) => r.ts_ms));
    set({
      screen: "session",
      loaded: true,
      durationMs,
      sessionElapsedSeconds: Math.floor(durationMs / 1000),
      sessionId: id,
      summary: sum[0]?.summary ?? "",
      notes: rows.map((r) => ({
        id: r.id, kind: r.kind, raw: r.raw, ts: r.ts_ms, status: "completed", expanded: r.expanded ?? undefined,
        owner: r.owner ?? undefined, due: r.due ?? undefined, done: !!r.done, auto: !!r.auto,
        citation: r.citation_quote ? { chunkIds: [], quote: r.citation_quote, start: r.citation_start ?? undefined } : undefined,
      })),
      transcript: chunks.map((r) => ({ id: r.chunk_id, speaker: r.speaker, text: r.text, ts: r.ts_ms })),
    });
  },

  dbError: null,
  dismissDbError: () => set({ dbError: null }),

  async deleteSession(id) {
    const d = await getDb();
    await d.execute("DELETE FROM sessions WHERE id=?", [id]); // notes and transcript go with it via ON DELETE CASCADE, atomically
    if (get().sessionId === id) set({ sessionId: null, loaded: false, durationMs: 0, sessionElapsedSeconds: 0, notes: [], transcript: [], summary: "" });
  },

  editNote(id, text) {
    typedKinds.add(id);
    editedNotes.add(id);
    set((s) => ({ notes: s.notes.map((n) => (n.id === id ? { ...n, expanded: text } : n)) }));
    persist((d) => d.execute("UPDATE notes SET expanded=? WHERE id=?", [text, id]));
  },

  async renameSession(id, title) {
    await (await getDb()).execute("UPDATE sessions SET title=? WHERE id=?", [title.trim() || null, id]);
  },

  deleteNote(id) {
    set((s) => ({ notes: s.notes.filter((n) => n.id !== id) }));
    persist((d) => d.execute("DELETE FROM notes WHERE id=?", [id]));
  },

  async backup() {
    const path = await invoke<string>("backup_path");
    await (await getDb()).execute("VACUUM INTO ?", [path]);
    await invoke("reveal_path", { path });
    return path;
  },

  setLevels: (levels) => set({ levels }),
  inspect: (focusChunk) => set({ focusChunk }),
}));
