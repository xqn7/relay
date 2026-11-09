// What plays without internet, and whether the internet is there. The backend pushes every
// change ("network-status", "storage-changed", "download-status"); nothing here polls.
import { listen } from "@tauri-apps/api/event";
import { api, type DownloadStatus, type Track } from "../api";
import { t } from "../i18n/index.svelte";
import { dialogs } from "./dialogs.svelte";
import { fail } from "./library.svelte";
import { ui } from "./ui.svelte";

const REFRESH_DELAY_MS = 400;
const SYNC_DELAY_MS = 1000;

class OfflineStore {
  online = $state(true);
  /** Songs saved for offline (in the downloads folder). */
  downloaded = $state<Set<string>>(new Set());
  /** Songs in the automatic song cache. */
  cached = $state<Set<string>>(new Set());
  /** This account's own saves: single songs and whole playlists. */
  savedSongs = $state<Set<string>>(new Set());
  savedPlaylists = $state<Set<string>>(new Set());
  status = $state<DownloadStatus>({ active: false, done: 0, total: 0, full: false, failed: 0 });
  private attached = false;
  private refreshTimer = 0;
  private syncTimer = 0;

  async attach(): Promise<void> {
    if (this.attached) return;
    this.attached = true;
    await listen<boolean>("network-status", ({ payload }) => this.setOnline(payload));
    await listen("storage-changed", () => this.scheduleRefresh());
    await listen<DownloadStatus>("download-status", ({ payload }) => (this.status = payload));
    this.online = await api.netStatus().catch(() => true);
    this.status = await api.downloadStatus().catch(() => this.status);
  }

  async refresh(): Promise<void> {
    const [avail, marks] = await Promise.all([api.storageAvailable(), api.offlineMarks()]);
    this.downloaded = new Set(avail.downloaded);
    this.cached = new Set(avail.cached);
    this.savedSongs = new Set(marks.songs);
    this.savedPlaylists = new Set(marks.playlists);
  }

  reset(): void {
    this.savedSongs = new Set();
    this.savedPlaylists = new Set();
  }

  private scheduleRefresh(): void {
    clearTimeout(this.refreshTimer);
    this.refreshTimer = window.setTimeout(() => this.refresh().catch((e) => console.error("offline refresh failed", e)), REFRESH_DELAY_MS);
  }

  private setOnline(online: boolean): void {
    if (online === this.online) return;
    this.online = online;
    if (online) ui.toast(t("offline.back"), "cloud");
    else ui.toast(t("offline.lost"), "alert", { label: t("offline.details"), run: () => dialogs.open({ kind: "connection" }) });
  }

  /** Plays without internet: Local music, downloads and the song cache. */
  isAvailable(track: Track): boolean {
    return track.source === "local" || this.downloaded.has(track.id) || this.cached.has(track.id);
  }

  isDownloaded(track: Track): boolean {
    return track.source === "local" || this.downloaded.has(track.id);
  }

  isSaved(track: Track): boolean {
    return this.savedSongs.has(track.id);
  }

  /** Lists show only playable songs while offline. */
  playable<T>(items: T[], track: (item: T) => Track): T[] {
    return this.online ? items : items.filter((i) => this.isAvailable(track(i)));
  }

  async setSaved(tracks: Track[], on: boolean): Promise<void> {
    const list = tracks.filter((x) => x.source !== "local");
    if (list.length === 0) return;
    try {
      await api.offlineSet(list, on);
      const next = new Set(this.savedSongs);
      for (const x of list) on ? next.add(x.id) : next.delete(x.id);
      this.savedSongs = next;
      ui.toast(on ? t("offline.saving", { n: list.length }) : t("offline.removed", { n: list.length }), "download");
    } catch (e) {
      fail(e);
    }
  }

  async setPlaylistSaved(id: string, on: boolean): Promise<void> {
    try {
      await api.playlistOfflineSet(id, on);
      const next = new Set(this.savedPlaylists);
      if (on) next.add(id);
      else next.delete(id);
      this.savedPlaylists = next;
      ui.toast(on ? t("offline.playlistSaving") : t("offline.playlistRemoved"), "download");
    } catch (e) {
      fail(e);
    }
  }

  /** After library changes, downloads catch up (new liked songs, songs added to saved playlists). */
  sync(): void {
    clearTimeout(this.syncTimer);
    this.syncTimer = window.setTimeout(() => api.offlineSync().catch(() => {}), SYNC_DELAY_MS);
  }
}

export const offline = new OfflineStore();
