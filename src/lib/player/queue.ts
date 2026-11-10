// Pure queue logic: no audio, no UI. Every operation returns a new state, so it's easy to test
// and to persist.
import type { Track } from "../api";

export type Repeat = "off" | "all" | "one";

export interface QueueItem {
  /** Unique per entry, so the same song can be queued twice. */
  uid: string;
  track: Track;
}

export interface QueueState {
  items: QueueItem[];
  /** Index of the current song, or -1 when nothing is loaded. */
  index: number;
  shuffle: boolean;
  repeat: Repeat;
  /** Order (uids) before shuffle was turned on, to restore it afterwards. */
  unshuffled: string[] | null;
  /** Radio mode: the title of the song it started from. The queue refills itself and
   * repeat-all is off. */
  radio: string | null;
}

export const emptyQueue: QueueState = { items: [], index: -1, shuffle: false, repeat: "off", unshuffled: null, radio: null };

let counter = 0;
export function makeItem(track: Track): QueueItem {
  counter += 1;
  return { uid: `${Date.now().toString(36)}-${counter}`, track };
}

export function current(q: QueueState): QueueItem | null {
  return q.items[q.index] ?? null;
}

export function upcoming(q: QueueState): QueueItem[] {
  return q.items.slice(q.index + 1);
}

function shuffled<T>(list: T[], random: () => number): T[] {
  const out = list.slice();
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

/** Replaces the queue with `tracks`, starting at `start`. With shuffle on, the rest is shuffled.
 * A new queue ends radio mode. */
export function playNow(q: QueueState, tracks: Track[], start = 0, random = Math.random): QueueState {
  if (tracks.length === 0) return q;
  const items = tracks.map(makeItem);
  const first = items[Math.min(Math.max(start, 0), items.length - 1)];
  if (!q.shuffle) return { ...q, items, index: items.indexOf(first), unshuffled: null, radio: null };
  const rest = shuffled(items.filter((i) => i !== first), random);
  return { ...q, items: [first, ...rest], index: 0, unshuffled: items.map((i) => i.uid), radio: null };
}

/** Starts radio mode from the current song: repeat-all would loop a list meant to go on and on. */
export function startRadio(q: QueueState, seedTitle: string): QueueState {
  return { ...q, radio: seedTitle, repeat: q.repeat === "all" ? "off" : q.repeat };
}

export function playNext(q: QueueState, tracks: Track[]): QueueState {
  if (q.index < 0) return addToQueue(q, tracks);
  const added = tracks.map(makeItem);
  const items = [...q.items.slice(0, q.index + 1), ...added, ...q.items.slice(q.index + 1)];
  const unshuffled = q.unshuffled ? insertAfter(q.unshuffled, q.items[q.index].uid, added.map((a) => a.uid)) : null;
  return { ...q, items, unshuffled };
}

export function addToQueue(q: QueueState, tracks: Track[]): QueueState {
  const added = tracks.map(makeItem);
  const items = [...q.items, ...added];
  return {
    ...q,
    items,
    index: q.index < 0 ? q.items.length : q.index,
    unshuffled: q.unshuffled ? [...q.unshuffled, ...added.map((a) => a.uid)] : null,
  };
}

function insertAfter(list: string[], anchor: string, added: string[]): string[] {
  const at = list.indexOf(anchor);
  return at < 0 ? [...list, ...added] : [...list.slice(0, at + 1), ...added, ...list.slice(at + 1)];
}

export function remove(q: QueueState, uid: string): QueueState {
  const at = q.items.findIndex((i) => i.uid === uid);
  if (at < 0) return q;
  const items = q.items.filter((i) => i.uid !== uid);
  let index = q.index;
  if (at < q.index) index -= 1;
  else if (at === q.index) index = Math.min(q.index, items.length - 1);
  return { ...q, items, index, unshuffled: q.unshuffled?.filter((u) => u !== uid) ?? null };
}

/** Moves an item; the current song stays current wherever it ends up. */
export function move(q: QueueState, from: number, to: number): QueueState {
  if (from === to || from < 0 || to < 0 || from >= q.items.length || to >= q.items.length) return q;
  const playing = q.items[q.index];
  const items = q.items.slice();
  const [item] = items.splice(from, 1);
  items.splice(to, 0, item);
  return { ...q, items, index: playing ? items.indexOf(playing) : q.index };
}

/** "Clear queue" keeps the song that's playing and drops everything else. */
export function clearUpcoming(q: QueueState): QueueState {
  const now = current(q);
  return now ? { ...q, items: [now], index: 0, unshuffled: q.unshuffled ? [now.uid] : null } : emptyQueue;
}

export function setShuffle(q: QueueState, on: boolean, random = Math.random): QueueState {
  if (on === q.shuffle) return q;
  const now = current(q);
  if (on) {
    const before = q.items.slice(0, q.index + 1);
    const after = shuffled(q.items.slice(q.index + 1), random);
    return { ...q, shuffle: true, items: [...before, ...after], unshuffled: q.items.map((i) => i.uid) };
  }
  if (!q.unshuffled) return { ...q, shuffle: false };
  const byUid = new Map(q.items.map((i) => [i.uid, i]));
  const restored = q.unshuffled.map((u) => byUid.get(u)).filter((i): i is QueueItem => !!i);
  const extra = q.items.filter((i) => !q.unshuffled!.includes(i.uid));
  const items = [...restored, ...extra];
  return { ...q, shuffle: false, items, index: now ? items.indexOf(now) : q.index, unshuffled: null };
}

/** Off → all → one → off; in radio mode "all" is skipped. */
export function cycleRepeat(r: Repeat, radio = false): Repeat {
  return r === "off" ? (radio ? "one" : "all") : r === "all" ? "one" : "off";
}

/**
 * Where to go after the current song. `auto` means the song ended by itself (repeat-one
 * applies); a manual "next" always moves on. Returns null when playback should stop.
 */
export function nextIndex(q: QueueState, auto: boolean, skip?: (item: QueueItem) => boolean): number | null {
  if (q.items.length === 0) return null;
  if (auto && q.repeat === "one") return q.index;
  const step = (i: number): number | null => (i + 1 < q.items.length ? i + 1 : q.repeat === "off" ? null : 0);
  let i = step(q.index);
  // Songs the user said not to play are passed over when the queue moves on by itself.
  for (let tries = 0; auto && skip && i != null && skip(q.items[i]) && tries < q.items.length; tries++) i = step(i);
  return i != null && auto && skip?.(q.items[i]) ? null : i;
}

export function prevIndex(q: QueueState): number {
  if (q.index > 0) return q.index - 1;
  return q.repeat === "all" && q.items.length > 0 ? q.items.length - 1 : 0;
}

/** Validates a queue loaded from disk; anything malformed is dropped, never trusted. */
export function restore(raw: unknown): QueueState | null {
  if (typeof raw !== "object" || raw === null) return null;
  const r = raw as Record<string, unknown>;
  if (!Array.isArray(r.items)) return null;
  const items = r.items.filter(
    (i): i is QueueItem =>
      typeof i === "object" && i !== null && typeof i.uid === "string" && typeof i.track?.id === "string" && typeof i.track?.title === "string",
  );
  const index = typeof r.index === "number" && r.index >= 0 && r.index < items.length ? r.index : items.length ? 0 : -1;
  const repeat: Repeat = r.repeat === "all" || r.repeat === "one" ? r.repeat : "off";
  const unshuffled = Array.isArray(r.unshuffled) ? r.unshuffled.filter((u): u is string => typeof u === "string") : null;
  const radio = typeof r.radio === "string" && r.radio.length < 1024 ? r.radio : null;
  return { items, index, shuffle: r.shuffle === true, repeat: radio && repeat === "all" ? "off" : repeat, unshuffled: r.shuffle === true ? unshuffled : null, radio };
}
