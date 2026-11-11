// User preferences with defaults (Liner's where Liner has the same setting). Every change is
// saved immediately. Keys listed in DEVICE_KEYS belong to the PC, everything else to the account.
import { api, type TranslateLang, type VisualQuality } from "../api";
import { validEq, validUserPresets, type EqState, type UserPreset } from "../audio/eq";

export type LyricsStyle = "karaoke" | "glow" | "bounce" | "fade";
export const LYRICS_STYLES: LyricsStyle[] = ["karaoke", "glow", "bounce", "fade"];
export const IDLE_MINUTES = [1, 2, 3, 5, 10];
export const SPEED_MIN = 0.5;
export const SPEED_MAX = 2;
export const PITCH_RANGE = 12;
export type BackgroundMode = "cover" | "image" | "off";
export type MiniPosition = "left" | "center" | "right";
export const MINI_POSITIONS: MiniPosition[] = ["left", "center", "right"];
/** Where a custom background image is used. The mini player arrives in Phase 7 and reads it then. */
export type BackgroundPlace = "app" | "fullscreen" | "idle" | "mini";
export const BACKGROUND_PLACES: BackgroundPlace[] = ["app", "fullscreen", "idle", "mini"];

/** What each output device remembers: its EQ and volume. Keyed by the device's name. */
export interface DeviceProfile {
  eq: EqState;
  volume: number;
}

export const defaults = {
  autoplay: true,
  doubleClick: "play" as "play" | "queue",
  startup: "resume" as "resume" | "empty",
  pauseOnDeviceChange: true,
  crossfade: 3,
  normalize: true,
  quality: "best" as "best" | "dataSaver",
  /** GB; 0 = no limit. */
  maxCacheGb: 3,
  downloadLimitGb: 5,
  autoDownloadLiked: false,
  showCachedInLibrary: true,
  autoBackup: false,
  backupFolder: "",
  lyricsStyle: "karaoke" as LyricsStyle,
  translateLyrics: false,
  translateTo: "en" as TranslateLang,
  upNext: true,
  /** Minutes without input before the idle screen shows. */
  idleMinutes: 3,
  /** Playback speed, pitch kept. */
  speed: 1,
  /** Semitones. */
  pitch: 0,
  sponsorBlock: true,
  waveform: false,
  visualizer: false,
  visualizerStyle: "bars" as "bars" | "wave",
  /** Output device by name; "" = the Windows default. */
  outputDevice: "",
  deviceProfiles: {} as Record<string, DeviceProfile>,
  eqPresets: [] as UserPreset[],
  /** Cover colors (moving backdrop), a custom image, or a plain static dark background. */
  background: "cover" as BackgroundMode,
  backgroundId: "",
  backgroundWhere: [...BACKGROUND_PLACES] as BackgroundPlace[],
  /** "auto" follows the recommendation for this PC's hardware. */
  visualQuality: "auto" as "auto" | VisualQuality,
  gameMode: false,
  miniPlayer: true,
  miniPosition: "center" as MiniPosition,
  miniShortcut: "Alt+Shift+O",
  miniAutoShow: false,
  miniOnTop: true,
  desktopLyrics: false,
  lyricsPosition: "bottom" as "bottom" | "top",
  lockScreen: false,
  discord: true,
  discordFormat: "title" as "title" | "artist" | "both",
  discordTimeline: true,
  discordArtwork: true,
  discordArtistBadge: false,
  discordShowPaused: false,
  discordButtonYoutube: true,
  discordButtonSoundcloud: true,
  discordButtonSpotify: true,
  startWithWindows: false,
  /** Started with Windows: carry on playing the last queue. */
  startPlay: false,
};

export type Settings = typeof defaults;
export type SettingKey = keyof Settings;

const DEVICE_KEYS = new Set<SettingKey>([
  "maxCacheGb",
  "downloadLimitGb",
  "autoDownloadLiked",
  "showCachedInLibrary",
  "autoBackup",
  "backupFolder",
  "outputDevice",
  "deviceProfiles",
  "eqPresets",
  // Desktop integration belongs to the PC: the backend reads these without an account.
  "visualQuality",
  "gameMode",
  "miniPlayer",
  "miniPosition",
  "miniShortcut",
  "miniAutoShow",
  "miniOnTop",
  "desktopLyrics",
  "lyricsPosition",
  "lockScreen",
  "discord",
  "discordFormat",
  "discordTimeline",
  "discordArtwork",
  "discordArtistBadge",
  "discordShowPaused",
  "discordButtonYoutube",
  "discordButtonSoundcloud",
  "discordButtonSpotify",
  "startWithWindows",
  "startPlay",
]);
export const CACHE_SIZES = [1, 3, 10, 0];
export const DOWNLOAD_SIZES = [2, 5, 20, 0];

function valid<K extends SettingKey>(key: K, value: unknown): value is Settings[K] {
  const d = defaults[key];
  if (typeof d !== typeof value) return false;
  if (key === "crossfade") return typeof value === "number" && value >= 0 && value <= 12;
  if (key === "doubleClick") return value === "play" || value === "queue";
  if (key === "startup") return value === "resume" || value === "empty";
  if (key === "quality") return value === "best" || value === "dataSaver";
  if (key === "maxCacheGb") return CACHE_SIZES.includes(value as number);
  if (key === "downloadLimitGb") return DOWNLOAD_SIZES.includes(value as number);
  if (key === "lyricsStyle") return LYRICS_STYLES.includes(value as LyricsStyle);
  if (key === "translateTo") return value === "en" || value === "de";
  if (key === "idleMinutes") return IDLE_MINUTES.includes(value as number);
  if (key === "backupFolder") return typeof value === "string" && value.length < 1024;
  if (key === "speed") return typeof value === "number" && value >= SPEED_MIN && value <= SPEED_MAX;
  if (key === "pitch") return Number.isInteger(value) && Math.abs(value as number) <= PITCH_RANGE;
  if (key === "visualizerStyle") return value === "bars" || value === "wave";
  if (key === "outputDevice") return typeof value === "string" && value.length < 300;
  if (key === "eqPresets") return validUserPresets(value);
  if (key === "deviceProfiles") return validProfiles(value);
  if (key === "background") return value === "cover" || value === "image" || value === "off";
  if (key === "backgroundId") return typeof value === "string" && value.length <= 64;
  if (key === "backgroundWhere") return Array.isArray(value) && value.length <= 4 && value.every((p) => BACKGROUND_PLACES.includes(p));
  if (key === "visualQuality") return ["auto", "high", "medium", "low"].includes(value as string);
  if (key === "miniPosition") return MINI_POSITIONS.includes(value as MiniPosition);
  if (key === "miniShortcut") return typeof value === "string" && validShortcut(value);
  if (key === "lyricsPosition") return value === "bottom" || value === "top";
  if (key === "discordFormat") return value === "title" || value === "artist" || value === "both";
  return true;
}

/** Same rule as the backend: Ctrl, Alt or Win plus one letter, digit, F-key or Space. */
export function validShortcut(text: string): boolean {
  const parts = text.split("+").map((p) => p.trim().toLowerCase());
  const mods = new Set(["ctrl", "alt", "shift", "win"]);
  const keys = parts.filter((p) => !mods.has(p));
  if (keys.length !== 1 || !parts.some((p) => p === "ctrl" || p === "alt" || p === "win")) return false;
  return /^([a-z0-9]|space|f([1-9]|1[0-9]|2[0-4]))$/.test(keys[0]);
}

function validProfiles(v: unknown): v is Record<string, DeviceProfile> {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return false;
  const entries = Object.entries(v);
  return (
    entries.length <= 40 &&
    entries.every(([k, p]) => k.length < 300 && typeof p === "object" && p !== null && validEq(p.eq) && typeof p.volume === "number" && p.volume >= 0 && p.volume <= 1)
  );
}

class SettingsStore {
  values = $state<Settings>({ ...defaults });

  async load(): Promise<void> {
    const saved = await api.settingsLoad();
    const next: Settings = { ...defaults };
    for (const key of Object.keys(defaults) as SettingKey[]) {
      const raw = DEVICE_KEYS.has(key) ? saved.device[key] : saved.account[key];
      if (valid(key, raw)) (next as Record<SettingKey, unknown>)[key] = raw;
    }
    this.values = next;
  }

  reset(): void {
    this.values = { ...defaults };
  }

  set<K extends SettingKey>(key: K, value: Settings[K]): void {
    if (!valid(key, value)) return;
    this.values[key] = value;
    api.settingSet(DEVICE_KEYS.has(key) ? "device" : "account", key, value).catch((e) => console.error("saving setting failed", key, e));
  }
}

export const settings = new SettingsStore();
