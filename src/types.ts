export type NoteKind = "decision" | "action" | "risk" | "discussion";
export type Speaker = "You" | "Remote";
export type NoteStatus = "pending" | "completed" | "failed" | "suggested";

export interface TranscriptChunk {
  id: number;
  speaker: Speaker;
  text: string;
  /** ms since session start */
  ts: number;
  /** start time in seconds */
  start?: number;
  /** end time in seconds */
  end?: number;
}

export interface Note {
  id: string;
  kind: NoteKind;
  raw: string;
  /** ms since session start */
  ts: number;
  status: NoteStatus;
  expanded?: string;
  owner?: string;
  due?: string;
  done?: boolean;
  citation?: {
    chunkIds: number[];
    quote: string;
    start?: number;
    end?: number;
  };
}

export interface NoteExpandedPayload {
  id: string;
  enhanced_text: string;
  raw_shorthand: string;
  citation_start: number;
  citation_end: number;
  citation_quote: string;
}

export interface AudioLevels {
  mic: number;
  system: number;
}

export function parseShorthand(line: string): { kind: NoteKind; text: string; owner?: string; due?: string } {
  const m = /^(dec|act|risk):\s*(.*)$/i.exec(line.trim());
  if (!m) return { kind: "discussion", text: line.trim() };
  const p = m[1].toLowerCase();
  const body = m[2];
  if (p === "dec") return { kind: "decision", text: body };
  if (p === "risk") return { kind: "risk", text: body };
  // act: <owner> <task> <due>
  const [owner, ...rest] = body.split(/\s+/);
  let task = rest.join(" ");
  let due: string | undefined;
  const d = /\s+(?:by|due|on)?\s*((?:\d{4}-\d{2}-\d{2})|(?:\d{1,2}\/\d{1,2})|(?:mon|tue|wed|thu|fri|sat|sun)\w*|tomorrow|eod|eow)$/i.exec(task);
  if (d) {
    due = d[1];
    task = task.slice(0, d.index);
  }
  return { kind: "action", text: task, owner, due };
}

export const fmtClock = (ms: number) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(Math.floor(s / 3600))}:${p(Math.floor((s % 3600) / 60))}:${p(s % 60)}`;
};
