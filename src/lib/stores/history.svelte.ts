// Play counts and the private session. Plays are recorded by the player; a private session
// records nothing (the backend refuses too) and ends when the app restarts.
import { listen } from "@tauri-apps/api/event";
import { api, type Track } from "../api";
import { showError } from "../errors";
import { t } from "../i18n/index.svelte";
import { ui } from "./ui.svelte";

class HistoryStore {
  /** Plays per track id. Replaced as a whole on change, so it isn't deeply tracked. */
  counts = $state.raw<Record<string, number>>({});
  private = $state(false);
  /** Bumped after each recorded play, so Home can refresh its lists. */
  version = $state(0);
  private attached = false;

  async attach(): Promise<void> {
    if (this.attached) return;
    this.attached = true;
    await listen<boolean>("private-changed", ({ payload }) => (this.private = payload === true));
  }

  async refresh(): Promise<void> {
    const [counts, priv] = await Promise.all([api.playCounts(), api.privateGet()]);
    this.counts = counts;
    this.private = priv;
  }

  reset(): void {
    this.counts = {};
    this.private = false;
  }

  plays(track: Track): number {
    return this.counts[track.id] ?? 0;
  }

  /** Counts a play; the id is used to update how long it was listened to. */
  async record(track: Track, ms: number): Promise<string | null> {
    if (this.private) return null;
    const id = await api.playRecord(track, ms);
    if (id) {
      this.counts = { ...this.counts, [track.id]: (this.counts[track.id] ?? 0) + 1 };
      this.version += 1;
    }
    return id;
  }

  async setPrivate(on: boolean): Promise<void> {
    try {
      await api.privateSet(on);
      this.private = on;
      ui.toast(on ? t("private.on") : t("private.off"), on ? "lock" : "check");
    } catch (e) {
      showError(t("errctx.private"), e);
    }
  }
}

export const history = new HistoryStore();
