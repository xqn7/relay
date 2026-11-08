// Typed wrappers around the backend commands. The backend validates everything again.
import { invoke } from "@tauri-apps/api/core";

export type SourceId = "youtube" | "soundcloud";
export const SOURCES: SourceId[] = ["youtube", "soundcloud"];

export interface ArtistRef {
  id: string | null;
  name: string;
}

export interface Track {
  id: string;
  source: SourceId;
  sourceId: string;
  title: string;
  artists: ArtistRef[];
  album: { id: string; name: string } | null;
  durationMs: number | null;
  cover: string | null;
  isVideo: boolean;
  link: string | null;
}

export interface Collection {
  source: SourceId;
  kind: "album" | "playlist";
  id: string;
  title: string;
  subtitle: string;
  cover: string | null;
}

export interface ArtistCard {
  source: SourceId;
  id: string;
  name: string;
  avatar: string | null;
}

export interface SearchResults {
  source: SourceId;
  tracks: Track[];
  albums: Collection[];
  artists: ArtistCard[];
  playlists: Collection[];
}

export type SearchKind = "all" | "songs" | "albums" | "artists" | "playlists";
export type Quality = "best" | "dataSaver";

export interface Account {
  id: string;
  email: string;
  displayName: string;
}

export interface Prepared {
  url: string;
  loudnessDb: number | null;
  standIn: Track | null;
}

/** Error shape returned by every failing command. */
export interface AppError {
  code: string;
  detail: string;
}

export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "code" in e;
}

export const api = {
  authState: () => invoke<Account | null>("auth_state"),
  signUp: (email: string, password: string, keep: boolean) =>
    invoke<{ account: Account; recoveryKey: string }>("sign_up", { email, password, keep }),
  signIn: (email: string, password: string, keep: boolean) => invoke<Account>("sign_in", { email, password, keep }),
  resetPassword: (email: string, recoveryKey: string, newPassword: string) =>
    invoke<Account>("reset_password", { email, recoveryKey, newPassword }),
  signOut: () => invoke<void>("sign_out"),

  settingsLoad: () => invoke<{ account: Record<string, unknown>; device: Record<string, unknown> }>("settings_load"),
  settingSet: (scope: "account" | "device", key: string, value: unknown) => invoke<void>("setting_set", { scope, key, value }),

  search: (source: SourceId, query: string, kind: SearchKind) => invoke<SearchResults>("search", { source, query, kind }),
  searchHistory: () => invoke<string[]>("search_history"),
  searchHistoryAdd: (query: string) => invoke<void>("search_history_add", { query }),
  searchHistoryClear: () => invoke<void>("search_history_clear"),
  collectionTracks: (source: SourceId, kind: string, id: string) => invoke<Track[]>("collection_tracks", { source, kind, id }),
  autoplayTracks: (track: Track) => invoke<Track[]>("autoplay_tracks", { track }),

  prepareTrack: (track: Track, quality: Quality, retry = false) => invoke<Prepared>("prepare_track", { track, quality, retry }),
  prefetchTrack: (track: Track, quality: Quality) => invoke<void>("prefetch_track", { track, quality }),

  queueSave: (queue: unknown) => invoke<void>("queue_save", { queue }),
  queueLoad: () => invoke<unknown>("queue_load"),

  mediaTrack: (nowPlaying: { title: string; artist: string; album: string | null; cover: string | null; durationMs: number }) =>
    invoke<void>("media_track", { nowPlaying }),
  mediaState: (playing: boolean, positionMs: number, durationMs: number) =>
    invoke<void>("media_state", { playing, positionMs: Math.max(0, Math.round(positionMs)), durationMs: Math.max(0, Math.round(durationMs)) }),

  openExternal: (url: string) => invoke<void>("open_external", { url }),
  exitApp: () => invoke<void>("exit_app"),
};

export function artistLine(t: Pick<Track, "artists">): string {
  return t.artists.map((a) => a.name).join(", ");
}

/** Through the app's cover proxy, which adds the CORS headers needed to read pixels. */
export function proxiedCover(url: string): string {
  return `http://relay.localhost/img/${encodeURIComponent(url)}`;
}

/** Asks Google/SoundCloud image servers for a smaller variant where the URL format allows it. */
export function coverAt(url: string | null, size: number): string | null {
  if (!url) return null;
  if (/googleusercontent\.com|ggpht\.com/.test(url)) return url.replace(/=w\d+-h\d+[^&?#]*$/, `=w${size}-h${size}-l90-rj`);
  if (url.includes("sndcdn.com") && size <= 120) return url.replace("-t500x500.", "-t120x120.");
  return url;
}

export function formatTime(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms) || ms < 0) return "--:--";
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}
