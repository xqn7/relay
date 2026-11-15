// Typed wrappers around the backend commands. The backend validates everything again.
import { invoke } from "@tauri-apps/api/core";

/** Sources the search can query. */
export type SearchSource = "youtube" | "soundcloud";
export const SOURCES: SearchSource[] = ["youtube", "soundcloud"];
/** Every source a song can come from. Spotify, Deezer and Apple Music songs play through a match. */
export type SourceId = SearchSource | "spotify" | "deezer" | "apple" | "local";
export const IMPORT_ONLY: ReadonlySet<string> = new Set(["spotify", "deezer", "apple"]);

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
  /** Imported songs: the source their audio plays from, or "none" when nothing matched. */
  via?: SourceId | "none";
  /** Plays or views on the song's service, when it says. */
  listens?: number;
}

export interface Collection {
  source: SearchSource;
  /** "track" only for SoundCloud singles on artist pages. */
  kind: "album" | "playlist" | "track";
  id: string;
  title: string;
  subtitle: string;
  cover: string | null;
  year: number | null;
}

export interface ArtistCard {
  source: SearchSource;
  id: string;
  name: string;
  avatar: string | null;
}

export interface SearchResults {
  source: SearchSource;
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
  avatar: string | null;
  banner: string | null;
}

export interface Prepared {
  url: string;
  loudnessDb: number | null;
  standIn: Track | null;
  via: Track | null;
}

export interface PlaylistSummary {
  id: string;
  name: string;
  cover: string | null;
  folderId: string | null;
  count: number;
  durationMs: number;
  origin: string | null;
  originUrl: string | null;
  plays: number;
}

export interface Folder {
  id: string;
  name: string;
  parentId: string | null;
}

export interface Overview {
  playlists: PlaylistSummary[];
  folders: Folder[];
  likedCount: number;
  followsCount: number;
}

export interface Item {
  itemId: string;
  addedAt: number;
  track: Track;
}

export interface Skipped {
  title: string;
  artist: string;
  reason: "deleted" | "private" | "unavailable" | "no_match";
}

export interface PlaylistDetail {
  playlist: PlaylistSummary;
  items: Item[];
  skipped: Skipped[];
}

export interface Removed {
  itemId: string;
  trackId: string;
  position: number;
  addedAt: number;
}

export interface RemoteList {
  origin: string;
  service: string;
  kind: string;
  title: string;
  owner: string;
  cover: string | null;
  tracks: Track[];
  skipped: Skipped[];
  truncated: boolean;
}

export interface ImportDone {
  jobId: string;
  ok: boolean;
  cancelled: boolean;
  error: AppError | null;
  playlistId: string | null;
  title: string;
  total: number;
  added: number;
  removed: number;
  updated: boolean;
  unmatched: Track[];
  skipped: Skipped[];
  truncated: boolean;
}

export interface ImportProgress {
  jobId: string;
  phase: "reading" | "matching";
  done: number;
  total: number;
}

export interface Hidden {
  kind: "track" | "artist";
  value: string;
  label: string;
}

export interface Followed {
  source: SearchSource;
  artistId: string;
  name: string;
  avatar: string | null;
}

export interface Releases {
  artist: Followed;
  releases: Collection[];
  failed: boolean;
}

export interface AlbumPage {
  source: SearchSource;
  id: string;
  title: string;
  artists: ArtistRef[];
  cover: string | null;
  year: number | null;
  kind: "album" | "ep" | "single";
  tracks: Track[];
  link: string | null;
}

export interface ArtistPage {
  source: SearchSource;
  id: string;
  name: string;
  avatar: string | null;
  banner: string | null;
  about: string | null;
  topTracks: Track[];
  albums: Collection[];
  singles: Collection[];
  similar: ArtistCard[];
  link: string | null;
}

export interface TrashEntry {
  kind: "playlist" | "folder" | "local";
  id: string;
  name: string;
  detail: string;
  deletedAt: number;
}

export interface Usage {
  downloadsBytes: number;
  downloadsCount: number;
  likedBytes: number;
  importedBytes: number;
  cacheSongsBytes: number;
  cacheSongsCount: number;
  artworkBytes: number;
  lyricsBytes: number;
  freeBytes: number | null;
  averageSongBytes: number;
}

export interface DownloadStatus {
  active: boolean;
  done: number;
  total: number;
  full: boolean;
  failed: number;
}

export interface ConnectionCheck {
  id: "internet" | "youtube" | "soundcloud" | "spotify" | "covers" | "lyrics";
  ok: boolean;
  code: string;
  detail: string;
}

export interface LyricLine {
  timeMs: number;
  text: string;
}

export interface Lyrics {
  synced: LyricLine[] | null;
  plain: string | null;
  instrumental: boolean;
  source: "lrclib" | "youtube" | "user";
  credit: string | null;
}

export interface Translation {
  lines: string[];
  sourceLang: string | null;
}

export type TranslateLang = "en" | "de";

export interface Trim {
  startMs: number;
  endMs: number | null;
}

export type VersionKind = "slowed" | "slowedReverb" | "spedUp" | "nightcore" | "remix" | "live" | "acoustic" | "instrumental";

export interface Version {
  kind: VersionKind;
  track: Track;
}

export interface ExportDone {
  saved: number;
  failed: string[];
  folder: string;
  file: string | null;
}

export interface Counted {
  track: Track;
  plays: number;
}

export interface ArtistStat {
  name: string;
  plays: number;
  minutes: number;
  cover: string | null;
}

export interface HomeLists {
  recent: Track[];
  most: Counted[];
  topArtists: ArtistStat[];
}

export interface Mix {
  number: number;
  artists: string[];
  tracks: Track[];
}

export interface Suggestion {
  artist: ArtistCard;
  because: string;
}

export interface Stats {
  minutes: number;
  plays: number;
  songs: number;
  artists: number;
  topSongs: Counted[];
  topArtists: ArtistStat[];
  topDay: { day: number; minutes: number } | null;
  longestStreak: number;
  streakStart: number | null;
  firstPlay: number | null;
  peakHour: number | null;
}

export type PinKind = "playlist" | "album" | "artist";

export interface Pin {
  kind: PinKind;
  /** Catalog source for albums and artists; "" for playlists. */
  source: "" | SearchSource;
  id: string;
  title: string;
  subtitle: string;
  cover: string | null;
}

export interface Background {
  id: string;
  url: string;
  animated: boolean;
  /** Unchecked JSON from the database; read through `readLook`. */
  look: unknown;
}

/** Error shape returned by every failing command. */
/** A helper program's download (yt-dlp, ffmpeg). */
export interface ToolProgress {
  tool: "yt-dlp" | "ffmpeg";
  phase: "waiting" | "downloading" | "unpacking" | "ready" | "failed";
  done: number;
  total: number;
}

/** A song found by a line of its lyrics. */
export interface LyricHit {
  title: string;
  artist: string;
  album: string | null;
  durationMs: number | null;
  line: string;
}

export interface AppError {
  code: string;
  detail: string;
}

export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "code" in e;
}

/** Strips the derived "via" field (and Svelte proxies) before a track goes to the backend. */
function plain(t: Track): Track {
  return {
    id: t.id,
    source: t.source,
    sourceId: t.sourceId,
    title: t.title,
    artists: t.artists.map((a) => ({ id: a.id, name: a.name })),
    album: t.album ? { id: t.album.id, name: t.album.name } : null,
    durationMs: t.durationMs,
    cover: t.cover,
    isVideo: t.isVideo,
    link: t.link,
  };
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

  search: (source: SearchSource, query: string, kind: SearchKind) => invoke<SearchResults>("search", { source, query, kind }),
  searchHistory: () => invoke<string[]>("search_history"),
  searchHistoryAdd: (query: string) => invoke<void>("search_history_add", { query }),
  searchHistoryClear: () => invoke<void>("search_history_clear"),
  collectionTracks: (source: SearchSource, kind: string, id: string) => invoke<Track[]>("collection_tracks", { source, kind, id }),
  autoplayTracks: (track: Track) => invoke<Track[]>("autoplay_tracks", { track: plain(track) }),

  prepareTrack: (track: Track, quality: Quality, retry = false) => invoke<Prepared>("prepare_track", { track: plain(track), quality, retry }),
  prefetchTrack: (track: Track, quality: Quality) => invoke<void>("prefetch_track", { track: plain(track), quality }),
  warmTrack: (track: Track, quality: Quality) => invoke<boolean>("warm_track", { track: plain(track), quality }),

  queueSave: (queue: unknown) => invoke<void>("queue_save", { queue }),
  queueLoad: () => invoke<unknown>("queue_load"),

  mediaTrack: (nowPlaying: NowPlayingInfo) => invoke<void>("media_track", { nowPlaying }),
  mediaState: (playing: boolean, positionMs: number, durationMs: number) =>
    invoke<void>("media_state", { playing, positionMs: Math.max(0, Math.round(positionMs)), durationMs: Math.max(0, Math.round(durationMs)) }),

  openExternal: (url: string) => invoke<void>("open_external", { url }),
  exitApp: () => invoke<void>("exit_app"),

  desktopInfo: () => invoke<DesktopInfo>("desktop_info"),
  mainWindowShow: () => invoke<void>("main_window_show"),
  miniToggle: () => invoke<void>("mini_toggle"),
  lyricsUnlock: (unlocked: boolean) => invoke<void>("lyrics_unlock", { unlocked }),
  lyricsActive: (active: boolean) => invoke<void>("lyrics_active", { active }),
  toolsPrepare: () => invoke<ToolProgress[]>("tools_prepare"),
  uiLabels: (labels: UiLabels) => invoke<void>("ui_labels", { labels }),
  hardwareInfo: () => invoke<HardwareInfo>("hardware_info"),
  discordPreview: () => invoke<DiscordActivity | null>("discord_preview"),
  updateCheck: () => invoke<UpdateInfo | null>("update_check"),
  updateInstall: () => invoke<void>("update_install"),
  youtubeStatus: () => invoke<{ signedIn: boolean; expired: boolean }>("youtube_status"),
  youtubeSignIn: () => invoke<void>("youtube_sign_in"),
  youtubeSignOut: () => invoke<void>("youtube_sign_out"),
  youtubePlaylists: () => invoke<Collection[]>("youtube_playlists"),
  youtubePlaylistTracks: (id: string) => invoke<Track[]>("youtube_playlist_tracks", { id }),

  libraryOverview: () => invoke<Overview>("library_overview"),
  playlistGet: (id: string) => invoke<PlaylistDetail>("playlist_get", { id }),
  playlistCreate: (name: string, folderId: string | null = null, tracks: Track[] = []) =>
    invoke<string>("playlist_create", { name, folderId, tracks: tracks.map(plain) }),
  playlistRename: (id: string, name: string) => invoke<void>("playlist_rename", { id, name }),
  playlistSetFolder: (id: string, folderId: string | null) => invoke<void>("playlist_set_folder", { id, folderId }),
  playlistDelete: (id: string) => invoke<void>("playlist_delete", { id }),
  playlistAdd: (id: string, tracks: Track[]) => invoke<number>("playlist_add", { id, tracks: tracks.map(plain) }),
  playlistRemove: (id: string, itemIds: string[]) => invoke<Removed[]>("playlist_remove", { id, itemIds }),
  playlistRestoreItems: (id: string, rows: Removed[]) => invoke<void>("playlist_restore_items", { id, rows }),
  playlistMove: (id: string, itemId: string, to: number) => invoke<void>("playlist_move", { id, itemId, to }),
  playlistPlayed: (id: string) => invoke<void>("playlist_played", { id }),
  playlistMerge: (a: string, b: string, into: string | null, newName: string | null) =>
    invoke<string>("playlist_merge", { a, b, into, newName }),
  folderCreate: (name: string, parentId: string | null = null) => invoke<string>("folder_create", { name, parentId }),
  folderRename: (id: string, name: string) => invoke<void>("folder_rename", { id, name }),
  folderSetParent: (id: string, parentId: string | null) => invoke<void>("folder_set_parent", { id, parentId }),
  folderDelete: (id: string) => invoke<void>("folder_delete", { id }),

  likeSet: (track: Track, liked: boolean) => invoke<void>("like_set", { track: plain(track), liked }),
  likedList: () => invoke<Item[]>("liked_list"),
  likedIds: () => invoke<string[]>("liked_ids"),
  hiddenSet: (kind: "track" | "artist", value: string, label: string, on: boolean) => invoke<void>("hidden_set", { kind, value, label, on }),
  hiddenList: () => invoke<Hidden[]>("hidden_list"),
  followSet: (artist: Followed, on: boolean) => invoke<void>("follow_set", { artist, on }),
  followsList: () => invoke<Followed[]>("follows_list"),
  followedReleases: () => invoke<Releases[]>("followed_releases"),

  artistPage: (source: SearchSource, id: string) => invoke<ArtistPage>("artist_page", { source, id }),
  artistFind: (name: string) => invoke<ArtistCard | null>("artist_find", { name }),
  albumPage: (source: SearchSource, id: string) => invoke<AlbumPage>("album_page", { source, id }),
  soundcloudTrack: (id: string) => invoke<Track>("soundcloud_track", { id }),

  trashList: () => invoke<TrashEntry[]>("trash_list"),
  trashRestore: (kind: string, id: string) => invoke<void>("trash_restore", { kind, id }),
  trashDelete: (kind: string, id: string) => invoke<void>("trash_delete", { kind, id }),
  trashEmpty: () => invoke<void>("trash_empty"),
  trashPurgeExpired: () => invoke<void>("trash_purge_expired"),

  importPreview: (url: string) => invoke<RemoteList>("import_preview", { url }),
  importStart: (url: string) => invoke<string>("import_start", { url }),
  importCancel: (jobId: string) => invoke<void>("import_cancel", { jobId }),
  importLikedLinks: (text: string) => invoke<string>("import_liked_links", { text }),
  importLikedFile: () => invoke<string | null>("import_liked_file"),
  importM3uPick: () => invoke<string | null>("import_m3u_pick"),
  exportM3u: (id: string) => invoke<boolean>("export_m3u", { id }),

  matchCandidates: (track: Track, query: string | null) => invoke<Track[]>("match_candidates", { track: plain(track), query }),
  matchGet: (track: Track) => invoke<Track | null>("match_get", { track: plain(track) }),
  matchSet: (track: Track, matched: Track) => invoke<void>("match_set", { track: plain(track), matched: plain(matched) }),
  prematch: (tracks: Track[]) => invoke<void>("prematch", { tracks: tracks.map(plain) }),

  localList: () => invoke<Track[]>("local_list"),
  localAddDialog: () => invoke<boolean>("local_add_dialog"),
  localEdit: (sourceId: string, title: string, artist: string, pickCover: boolean) =>
    invoke<boolean>("local_edit", { sourceId, title, artist, pickCover }),
  localRemove: (sourceId: string) => invoke<void>("local_remove", { sourceId }),
  localFolder: () => invoke<string>("local_folder"),
  localFolderPick: () => invoke<string | null>("local_folder_pick"),
  localOpenFolder: () => invoke<void>("local_open_folder"),

  offlineMarks: () => invoke<{ songs: string[]; playlists: string[] }>("offline_marks"),
  offlineSet: (tracks: Track[], on: boolean) => invoke<void>("offline_set", { tracks: tracks.map(plain), on }),
  playlistOfflineSet: (id: string, on: boolean) => invoke<void>("playlist_offline_set", { id, on }),
  offlineSync: () => invoke<void>("offline_sync"),
  storageAvailable: () => invoke<{ downloaded: string[]; cached: string[] }>("storage_available"),
  offlineLists: () => invoke<{ downloaded: Track[]; cached: Track[] }>("offline_lists"),
  downloadStatus: () => invoke<DownloadStatus>("download_status"),
  storageUsage: () => invoke<Usage>("storage_usage"),
  storageClear: (what: "songs" | "artwork" | "lyrics" | "downloads") => invoke<void>("storage_clear", { what }),
  storageOpen: (what: "downloads" | "cache") => invoke<void>("storage_open", { what }),
  netStatus: () => invoke<boolean>("net_status"),
  netCheck: () => invoke<boolean>("net_check"),
  connectionCheck: () => invoke<ConnectionCheck[]>("connection_check"),
  logTail: () => invoke<string>("log_tail"),
  releaseNotes: () => invoke<string>("release_notes"),
  recoveryKeySave: (text: string) => invoke<boolean>("recovery_key_save", { text }),
  accountChangePassword: (currentPassword: string, newPassword: string) => invoke<void>("account_change_password", { currentPassword, newPassword }),
  accountNewRecoveryKey: (password: string) => invoke<string>("account_new_recovery_key", { password }),
  libraryDamaged: () => invoke<string | null>("library_damaged"),
  libraryRestoreNewest: () => invoke<number>("library_restore_newest"),
  historyClear: (range: "hour" | "day" | "week" | "all") => invoke<number>("history_clear", { range }),
  updateReady: () => invoke<UpdateInfo | null>("update_ready"),
  lyricsSearch: (query: string) => invoke<LyricHit[]>("lyrics_search", { query }),
  lyricsHitTrack: (hit: LyricHit) => invoke<Track>("lyrics_hit_track", { hit }),
  reportProblem: (message: string, picture: Uint8Array) => invoke<void>("report_problem", picture, { headers: { message: encodeURIComponent(message) } }),
  reportError: (what: string) => invoke<void>("report_error", { what }),
  backupCreate: () => invoke<boolean>("backup_create"),
  backupRestore: () => invoke<number>("backup_restore"),
  backupSuggestedFolder: () => invoke<string>("backup_suggested_folder"),
  backupFolderPick: () => invoke<string | null>("backup_folder_pick"),
  coverHd: (track: Track) => invoke<string | null>("cover_hd", { track: plain(track) }),
  coverSave: (url: string, name: string) => invoke<boolean>("cover_save", { url, name }),
  coverDragPrepare: (url: string, name: string) => invoke<void>("cover_drag_prepare", { url, name }),
  coverDragStart: () => invoke<void>("cover_drag_start"),
  localUseHdCover: (track: Track) => invoke<void>("local_use_hd_cover", { track: plain(track) }),

  lyricsGet: (track: Track) => invoke<Lyrics | null>("lyrics_get", { track: plain(track) }),
  lyricsPreload: (tracks: Track[], lang: TranslateLang | null) => invoke<void>("lyrics_preload", { tracks: tracks.map(plain), lang }),
  lyricsTranslate: (trackId: string, lang: TranslateLang) => invoke<Translation>("lyrics_translate", { trackId, lang }),
  lyricsSave: (track: Track, lines: LyricLine[], plainText: string) =>
    invoke<Lyrics>("lyrics_save", { track: plain(track), lines, plain: plainText }),
  lyricsReset: (track: Track) => invoke<Lyrics | null>("lyrics_reset", { track: plain(track) }),
  deeplKeySaved: () => invoke<boolean>("deepl_key_saved"),
  deeplKeySet: (key: string | null) => invoke<void>("deepl_key_set", { key }),

  playlistReplace: (id: string, itemId: string, track: Track) => invoke<void>("playlist_replace", { id, itemId, track: plain(track) }),
  trimsList: () => invoke<Record<string, Trim>>("trims_list"),
  trimSet: (track: Track, trim: Trim | null) => invoke<void>("trim_set", { track: plain(track), trim }),
  waveform: (track: Track) => invoke<number[]>("waveform", { track: plain(track) }),
  sponsorSegments: (videoId: string) => invoke<[number, number][]>("sponsor_segments", { videoId }),
  otherVersions: (track: Track) => invoke<Version[]>("other_versions", { track: plain(track) }),
  exportMp3: (tracks: Track[], startMs: number | null = null, endMs: number | null = null) =>
    invoke<ExportDone | null>("export_mp3", { tracks: tracks.map(plain), startMs, endMs }),
  exportReveal: () => invoke<void>("export_reveal"),

  playRecord: (track: Track, ms: number) => invoke<string | null>("play_record", { track: plain(track), ms: Math.round(ms) }),
  playListened: (id: string, ms: number) => invoke<void>("play_listened", { id, ms: Math.round(ms) }),
  playCounts: () => invoke<Record<string, number>>("play_counts"),
  homeLists: () => invoke<HomeLists>("home_lists"),
  recentPlays: () => invoke<Track[]>("recent_plays"),
  homeMixes: (today: string) => invoke<Mix[]>("home_mixes", { today }),
  discover: () => invoke<Suggestion[]>("discover"),
  listeningStats: (from: number, to: number) =>
    invoke<Stats>("listening_stats", { from: Math.floor(from), to: Math.floor(to), tzMinutes: -new Date().getTimezoneOffset() }),
  privateGet: () => invoke<boolean>("private_get"),
  privateSet: (on: boolean) => invoke<void>("private_set", { on }),
  // Images go as raw bytes (no JSON encoding); the extra details travel in headers.
  profileImageSet: (kind: "avatar" | "banner", bytes: Uint8Array) => invoke<Account>("profile_image_set", bytes, { headers: { kind } }),
  profileImageClear: (kind: "avatar" | "banner") => invoke<Account>("profile_image_clear", { kind }),
  profileNameSet: (name: string) => invoke<Account>("profile_name_set", { name }),
  imageSave: (png: Uint8Array, name: string) => invoke<boolean>("image_save", png, { headers: { name: encodeURIComponent(name) } }),
  backgroundsList: () => invoke<Background[]>("backgrounds_list"),
  backgroundAdd: (bytes: Uint8Array) => invoke<Background>("background_add", bytes),
  backgroundLookSet: (id: string, look: object) => invoke<void>("background_look_set", { id, look }),
  backgroundDelete: (id: string) => invoke<void>("background_delete", { id }),
  speedDialList: () => invoke<Pin[]>("speed_dial_list"),
  speedDialSet: (pin: Pin, on: boolean) => invoke<void>("speed_dial_set", { pin, on }),
  speedDialOrder: (keys: string[]) => invoke<void>("speed_dial_order", { keys }),
};

/** "1.2 GB", "340 MB". */
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = Math.max(0, bytes);
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const digits = i >= 3 && v < 10 ? 1 : 0;
  return `${v.toFixed(digits)} ${units[i]}`;
}

/** What the backend shows outside the window: media overlay, tray, Discord. */
export interface NowPlayingInfo {
  title: string;
  artist: string;
  album: string | null;
  cover: string | null;
  durationMs: number;
  artistId: string | null;
  links: { youtube: string | null; soundcloud: string | null; spotify: string | null };
}

export interface DesktopInfo {
  startedInTray: boolean;
  game: boolean;
  shortcutOk: boolean;
  miniCustom: boolean;
  lyricsCustom: boolean;
  lyricsUnlocked: boolean;
  discord: "off" | "waiting" | "connected";
  version: string;
  helperBlocked: boolean;
}

export interface UiLabels {
  play: string;
  pause: string;
  next: string;
  previous: string;
  privateSession: string;
  miniPlayer: string;
  unlockLyrics: string;
  lockLyrics: string;
  show: string;
  quit: string;
  paused: string;
  listenYoutube: string;
  listenSoundcloud: string;
  listenSpotify: string;
}

export type VisualQuality = "high" | "medium" | "low";

export interface HardwareInfo {
  hardware: {
    cpu: string;
    cores: number;
    ramGb: number;
    gpu: string;
    gpuIntegrated: boolean;
    width: number;
    height: number;
    refreshHz: number;
  };
  recommended: { quality: VisualQuality; reason: "weak" | "integratedSmall" | "integratedScreen" | "strong" };
}

/** Discord's activity as the backend would send it (shape from Discord's RPC). */
export interface DiscordActivity {
  details: string;
  state: string;
  timestamps?: { start: number; end: number };
  assets?: { large_image?: string; large_text?: string; small_image?: string; small_text?: string };
  buttons?: { label: string; url: string }[];
}

export interface UpdateInfo {
  version: string;
  notes: string;
}

export function artistLine(t: Pick<Track, "artists">): string {
  return t.artists.map((a) => a.name).join(", ");
}

/** Through the app's cover proxy, which adds the CORS headers needed to read pixels. */
export function proxiedCover(url: string): string {
  // Local music covers are served by the app itself, already with CORS headers.
  if (url.startsWith("http://relay.localhost/") || !url.startsWith("https://")) return url;
  return `http://relay.localhost/img/${encodeURIComponent(url)}`;
}

/** Asks Google/SoundCloud image servers for a smaller variant where the URL format allows it. */
export function coverAt(url: string | null, size: number): string | null {
  if (!url) return null;
  if (/googleusercontent\.com|ggpht\.com/.test(url)) return url.replace(/=w\d+-h\d+[^&?#]*$/, `=w${size}-h${size}-l90-rj`);
  if (url.includes("sndcdn.com") && size <= 120) return url.replace("-t500x500.", "-t120x120.");
  return url;
}

/** "1.2M", "34K": big counts as people read them, in the UI language. */
export function compactCount(n: number, lang: string): string {
  return new Intl.NumberFormat(lang, { notation: "compact", maximumFractionDigits: 1 }).format(n);
}

export function formatTime(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms) || ms < 0) return "--:--";
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/** Total length of a list: "3 hr 12 min" / "42 min". */
export function formatLength(ms: number, hourUnit: string): string {
  const min = Math.round(ms / 60000);
  const h = Math.floor(min / 60);
  const m = min % 60;
  if (h > 0) return m > 0 ? `${h} ${hourUnit} ${m} min` : `${h} ${hourUnit}`;
  return `${m} min`;
}
