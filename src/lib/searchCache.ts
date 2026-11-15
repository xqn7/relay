// The last few searches, kept for a minute: typing the same thing again, going back a word, or
// reopening search shows the results at once instead of asking the services again.
import type { SearchKind, SearchResults, SearchSource } from "./api";

const KEEP_MS = 60_000;
const MAX_ENTRIES = 30;

const entries = new Map<string, { at: number; data: SearchResults }>();

const key = (source: SearchSource, kind: SearchKind, query: string) => `${source}\u0001${kind}\u0001${query.trim().toLowerCase()}`;

export function cachedSearch(source: SearchSource, kind: SearchKind, query: string): SearchResults | null {
  const k = key(source, kind, query);
  const hit = entries.get(k);
  if (!hit) return null;
  if (Date.now() - hit.at > KEEP_MS) {
    entries.delete(k);
    return null;
  }
  return hit.data;
}

export function rememberSearch(source: SearchSource, kind: SearchKind, query: string, data: SearchResults): void {
  const k = key(source, kind, query);
  entries.delete(k);
  entries.set(k, { at: Date.now(), data });
  // A Map keeps insertion order, so the first key is the oldest.
  if (entries.size > MAX_ENTRIES) entries.delete(entries.keys().next().value!);
}
