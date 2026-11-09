// Turns timed lyrics lines into what the lyrics views show: lines with an estimated time for
// each word (LRC only times lines), "♪" markers in long instrumental parts, and search hits.
import type { LyricLine } from "../api";

export interface Word {
  text: string;
  startMs: number;
  endMs: number;
}

export interface ShownLine {
  /** Index in the source lines (translations line up with it); -1 for "♪" markers. */
  index: number;
  timeMs: number;
  /** When the last word should be fully sung. */
  endMs: number;
  text: string;
  words: Word[];
  /** An instrumental part, shown as "♪". */
  gap: boolean;
}

/** Roughly how long a sung character takes; good enough to sweep words in time. */
const MS_PER_CHAR = 62;
const MIN_SING_MS = 900;
/** An empty line this long (or a song starting this late) gets a "♪". */
const GAP_MS = 4000;
const INTRO_MS = 6000;
/** A sung line followed by this much silence gets a "♪" after it. */
const PAUSE_MS = 10_000;
const PAUSE_LEAD_MS = 1500;

const letters = (s: string) => s.replace(/\s+/g, "").length;

function gap(timeMs: number, endMs: number): ShownLine {
  return { index: -1, timeMs, endMs, text: "", words: [], gap: true };
}

/** Spreads a line's singing time over its words by their length. */
export function timeWords(text: string, startMs: number, singMs: number): Word[] {
  const parts = text.split(/\s+/).filter(Boolean);
  const weights = parts.map((w) => letters(w) + 1);
  const total = weights.reduce((a, b) => a + b, 0) || 1;
  let at = startMs;
  return parts.map((w, i) => {
    const len = (singMs * weights[i]) / total;
    const word = { text: w, startMs: Math.round(at), endMs: Math.round(at + len) };
    at += len;
    return word;
  });
}

export function buildLines(src: LyricLine[], durationMs: number): ShownLine[] {
  const out: ShownLine[] = [];
  if (src.length && src[0].timeMs >= INTRO_MS) out.push(gap(0, src[0].timeMs));
  src.forEach((line, i) => {
    const last = i === src.length - 1;
    const next = last ? Math.max(durationMs, line.timeMs + 5000) : src[i + 1].timeMs;
    const span = Math.max(0, next - line.timeMs);
    const text = line.text.trim();
    if (!text) {
      if (span >= GAP_MS && !last) out.push(gap(line.timeMs, next));
      return;
    }
    const sing = Math.min(span, Math.max(MIN_SING_MS, letters(text) * MS_PER_CHAR));
    out.push({ index: i, timeMs: line.timeMs, endMs: line.timeMs + sing, text, words: timeWords(text, line.timeMs, sing), gap: false });
    if (!last && span - sing >= PAUSE_MS) out.push(gap(line.timeMs + sing + PAUSE_LEAD_MS, next));
  });
  return out;
}

/** The line being sung at `posMs` (the last one that started), or -1 before the first. */
export function activeIndex(lines: ShownLine[], posMs: number): number {
  let lo = 0;
  let hi = lines.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid].timeMs <= posMs) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return found;
}

/** 0..1: how much of the word is sung. */
export function wordProgress(word: Word, posMs: number): number {
  if (posMs <= word.startMs) return 0;
  if (posMs >= word.endMs) return 1;
  return (posMs - word.startMs) / (word.endMs - word.startMs);
}

/** Lowercase and without accents, keeping a map back to the original characters. */
function folded(text: string): { norm: string; at: number[] } {
  let norm = "";
  const at: number[] = [];
  for (let i = 0; i < text.length; i++) {
    const f = text[i].normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
    for (let k = 0; k < f.length; k++) at.push(i);
    norm += f;
  }
  at.push(text.length);
  return { norm, at };
}

export function foldText(s: string): string {
  return folded(s).norm;
}

/** Splits text into plain and matching parts, for highlighting search hits. */
export function splitMatches(text: string, query: string): { text: string; hit: boolean }[] {
  const q = foldText(query.trim());
  if (!q) return [{ text, hit: false }];
  const { norm, at } = folded(text);
  const parts: { text: string; hit: boolean }[] = [];
  let from = 0;
  let i = norm.indexOf(q);
  while (i >= 0) {
    const start = at[i];
    const end = at[i + q.length];
    if (start > from) parts.push({ text: text.slice(from, start), hit: false });
    parts.push({ text: text.slice(start, end), hit: true });
    from = end;
    i = norm.indexOf(q, i + q.length);
  }
  if (from < text.length) parts.push({ text: text.slice(from), hit: false });
  return parts;
}
