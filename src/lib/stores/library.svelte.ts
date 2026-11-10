// The signed-in account's library as the UI sees it: playlists, folders, liked song ids,
// hidden songs/artists and followed artists. Refreshed by the backend's "library-changed"
// event, so every view stays in sync without polling.
import { listen } from "@tauri-apps/api/event";
import { api, artistLine, isAppError, type Followed, type Hidden, type Overview, type Track, type Trim } from "../api";
import { t } from "../i18n/index.svelte";
import { fold } from "../text";
import { offline } from "./offline.svelte";
import { ui } from "./ui.svelte";

const emptyOverview: Overview = { playlists: [], folders: [], likedCount: 0, followsCount: 0 };

class LibraryStore {
  overview = $state<Overview>(emptyOverview);
  liked = $state<Set<string>>(new Set());
  hidden = $state<Hidden[]>([]);
  follows = $state<Followed[]>([]);
  /** Songs with a custom start/end, by track id. */
  trims = $state<Record<string, Trim>>({});
  /** Bumped whenever the library changed, so open pages know to reload. */
  version = $state(0);
  private attached = false;

  async attach(): Promise<void> {
    if (this.attached) return;
    this.attached = true;
    await listen("library-changed", () => this.refresh());
  }

  async refresh(): Promise<void> {
    const [overview, liked, hidden, follows, trims] = await Promise.all([
      api.libraryOverview(),
      api.likedIds(),
      api.hiddenList(),
      api.followsList(),
      api.trimsList(),
    ]);
    this.trims = trims;
    this.overview = overview;
    this.liked = new Set(liked);
    this.hidden = hidden;
    this.follows = follows;
    this.version += 1;
    // New likes or playlist songs may need downloading (saved playlists, Liked Songs).
    offline.sync();
  }

  reset(): void {
    this.overview = emptyOverview;
    this.liked = new Set();
    this.hidden = [];
    this.follows = [];
    this.trims = {};
  }

  /** Saves or (with null) removes a trim. False when saving failed (already shown). */
  async setTrim(track: Track, trim: Trim | null): Promise<boolean> {
    try {
      await api.trimSet(track, trim);
      const next = { ...this.trims };
      if (trim) next[track.id] = trim;
      else delete next[track.id];
      this.trims = next;
      return true;
    } catch (e) {
      fail(e);
      return false;
    }
  }

  isLiked(track: Track): boolean {
    return this.liked.has(track.id);
  }

  async toggleLike(track: Track): Promise<void> {
    const on = !this.isLiked(track);
    const next = new Set(this.liked);
    if (on) next.add(track.id);
    else next.delete(track.id);
    this.liked = next;
    try {
      await api.likeSet(track, on);
      this.overview = { ...this.overview, likedCount: this.overview.likedCount + (on ? 1 : -1) };
      this.version += 1;
      offline.sync();
    } catch (e) {
      this.liked = new Set(on ? [...next].filter((id) => id !== track.id) : [...next, track.id]);
      fail(e);
    }
  }

  isHidden(track: Track): boolean {
    return this.hidden.some((h) =>
      h.kind === "track" ? h.value === track.id : track.artists.some((a) => fold(a.name) === h.value),
    );
  }

  async setHidden(kind: "track" | "artist", value: string, label: string, on: boolean): Promise<void> {
    try {
      await api.hiddenSet(kind, value, label, on);
      this.hidden = await api.hiddenList();
    } catch (e) {
      fail(e);
    }
  }

  async hideTrack(track: Track): Promise<void> {
    await this.setHidden("track", track.id, `${track.title} · ${artistLine(track)}`, true);
    ui.toast(t("toast.songHidden"), "eyeOff", { label: t("toast.undo"), run: () => this.setHidden("track", track.id, "", false) });
  }

  async hideArtist(name: string): Promise<void> {
    await this.setHidden("artist", name, name, true);
    ui.toast(t("toast.artistHidden", { name }), "eyeOff", { label: t("toast.undo"), run: () => this.setHidden("artist", fold(name), "", false) });
  }

  isFollowing(source: string, id: string): boolean {
    return this.follows.some((f) => f.source === source && f.artistId === id);
  }

  async setFollow(artist: Followed, on: boolean): Promise<void> {
    try {
      await api.followSet(artist, on);
      this.follows = await api.followsList();
    } catch (e) {
      fail(e);
    }
  }

  async createPlaylist(name: string, tracks: Track[] = [], folderId: string | null = null): Promise<string | null> {
    try {
      const id = await api.playlistCreate(name, folderId, tracks);
      await this.refresh();
      return id;
    } catch (e) {
      fail(e);
      return null;
    }
  }

  async addToPlaylist(id: string, tracks: Track[]): Promise<void> {
    const name = this.overview.playlists.find((p) => p.id === id)?.name ?? "";
    try {
      const added = await api.playlistAdd(id, tracks);
      ui.toast(added === 0 ? t("toast.alreadyInPlaylist", { name }) : t("toast.addedToPlaylist", { name, n: added }), "check");
      await this.refresh();
    } catch (e) {
      fail(e);
    }
  }

  async deletePlaylist(id: string): Promise<void> {
    const name = this.overview.playlists.find((p) => p.id === id)?.name ?? "";
    try {
      await api.playlistDelete(id);
      await this.refresh();
      ui.toast(t("toast.movedToTrash", { name }), "trash", {
        label: t("toast.undo"),
        run: () => api.trashRestore("playlist", id).then(() => this.refresh()),
      });
    } catch (e) {
      fail(e);
    }
  }

  async deleteFolder(id: string): Promise<void> {
    const name = this.overview.folders.find((f) => f.id === id)?.name ?? "";
    try {
      await api.folderDelete(id);
      await this.refresh();
      ui.toast(t("toast.movedToTrash", { name }), "trash", {
        label: t("toast.undo"),
        run: () => api.trashRestore("folder", id).then(() => this.refresh()),
      });
    } catch (e) {
      fail(e);
    }
  }

  async moveToFolder(playlistId: string, folderId: string | null): Promise<void> {
    try {
      await api.playlistSetFolder(playlistId, folderId);
      await this.refresh();
    } catch (e) {
      fail(e);
    }
  }

  async nestFolder(folderId: string, parentId: string | null): Promise<void> {
    try {
      await api.folderSetParent(folderId, parentId);
      await this.refresh();
    } catch (e) {
      fail(e);
    }
  }
}

/** Shows a human message for a failed library action. */
export function fail(e: unknown): void {
  console.error(e);
  const code = isAppError(e) ? e.code : "";
  const key = code === "too_deep" ? "error.tooDeep" : code === "offline" || code === "network" ? "error.offline" : "error.generic";
  ui.toast(t(key), "alert");
}

export const library = new LibraryStore();
