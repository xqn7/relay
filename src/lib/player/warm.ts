// Looks songs up a moment before they're likely played (the pointer rests on one or a button
// goes down, the first songs of a list that just opened), so the click finds the stream link
// ready and playback starts at once. The backend only resolves the link; nothing is downloaded.
import { api, type Track } from "../api";
import { settings } from "../stores/settings.svelte";

/** How long the pointer rests on a song before it's looked up: passing over a list doesn't count. */
const REST_MS = 150;
/** A looked-up link stays valid for hours; asking again within this time does nothing. */
const AGAIN_MS = 10 * 60_000;
const REMEMBER = 300;
/** Songs looked up when a list opens: the ones most likely to be clicked first. */
export const LIST_HEAD = 3;

const warmed = new Map<string, number>();
let timer = 0;

/** After the pointer rested a moment: the song under it, then the one below. */
export function warmSoon(...tracks: (Track | undefined)[]): void {
  clearTimeout(timer);
  timer = window.setTimeout(() => tracks.forEach((t) => t && warmNow(t)), REST_MS);
}

export function warmCancel(): void {
  clearTimeout(timer);
}

export function warmNow(track: Track): void {
  if (track.source === "local" || track.via === "none") return;
  const now = Date.now();
  const last = warmed.get(track.id);
  if (last && now - last < AGAIN_MS) return;
  if (warmed.size >= REMEMBER) warmed.clear();
  warmed.set(track.id, now);
  // Turned down when enough lookups already run; it can be asked again next time.
  api.warmTrack(track, settings.values.quality).then(
    (taken) => taken || warmed.delete(track.id),
    () => warmed.delete(track.id),
  );
}
