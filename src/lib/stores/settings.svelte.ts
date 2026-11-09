// User preferences with defaults (Liner's where Liner has the same setting). Every change is
// saved immediately. Keys listed in DEVICE_KEYS belong to the PC, everything else to the account.
import { api, type TranslateLang } from "../api";

export type LyricsStyle = "karaoke" | "glow" | "bounce" | "fade";
export const LYRICS_STYLES: LyricsStyle[] = ["karaoke", "glow", "bounce", "fade"];
export const IDLE_MINUTES = [1, 2, 3, 5, 10];

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
};

export type Settings = typeof defaults;
export type SettingKey = keyof Settings;

const DEVICE_KEYS = new Set<SettingKey>(["maxCacheGb", "downloadLimitGb", "autoDownloadLiked", "showCachedInLibrary", "autoBackup", "backupFolder"]);
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
  return true;
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
