// What the main window tells the mini player and desktop lyrics, and what they send back.
// The overlays never play audio; they show the main window's player and ask it to act.
import type { VisualQuality } from "../api";
import type { Look } from "../backdrop/look";
import type { ShownLine } from "../lyrics/lines";
import type { LyricsStyle } from "../stores/settings.svelte";

export const NOW_EVENT = "overlay-now";
export const LYRICS_EVENT = "overlay-lyrics";
/** An overlay just loaded and wants the current state. */
export const READY_EVENT = "overlay-ready";
export const REMOTE_EVENT = "overlay-remote";

export interface NowFeed {
  track: { id: string; title: string; artist: string; cover: string | null } | null;
  liked: boolean;
  playing: boolean;
  /** Position at `at` (Date.now()); overlays move it on themselves while playing. */
  posMs: number;
  at: number;
  rate: number;
  quality: VisualQuality;
  /** The custom background image, when it's meant for the mini player too. */
  background: { url: string; look: Look } | null;
}

export interface LyricsFeed {
  trackId: string | null;
  lines: ShownLine[];
  style: LyricsStyle;
  unlocked: boolean;
}

export type RemoteAction = "toggle" | "next" | "previous" | "like";

/** Where the song is now, from the last report. */
export function positionNow(feed: NowFeed, now = Date.now()): number {
  return feed.playing ? feed.posMs + (now - feed.at) * feed.rate : feed.posMs;
}
