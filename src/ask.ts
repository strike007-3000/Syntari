/** Words too common to search on. */
const STOP = new Set("a an and are as at be but by did do does for from had has have how i in is it its me my of on or our say said so than that the their them then there these they this to us was we were what when where which who why will with would you your about tell any all".split(" "));

/** An FTS5 MATCH expression for a question: its meaningful words, quoted and OR'd (the index stems them). Null when none are left. */
export function ftsQuery(question: string): string | null {
  const words = [...new Set((question.toLowerCase().match(/[\p{L}\p{N}]{3,}/gu) ?? []).filter((w) => !STOP.has(w)))].slice(0, 12);
  return words.length ? words.map((w) => `"${w}"`).join(" OR ") : null;
}

export interface Source {
  n: number;
  sessionId: number;
  chunkId?: number;
  title: string;
  startedAt: number;
  ts: number;
  speaker?: string;
  text: string;
}
