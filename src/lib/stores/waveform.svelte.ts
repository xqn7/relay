// The playing song's waveform for the seek bar, computed by the backend once per song
// (after the song is fully downloaded) and stored there.
import { api, isAppError, type Track } from "../api";
import { t } from "../i18n/index.svelte";
import { ui } from "./ui.svelte";

class WaveformStore {
  /** Loudness per bar, 0–255, for `trackId`. */
  bars = $state<number[] | null>(null);
  trackId = $state<string | null>(null);
  private wanted: string | null = null;
  private warned = false;

  async load(track: Track | null): Promise<void> {
    const id = track?.id ?? null;
    if (id === this.wanted) return;
    this.wanted = id;
    if (this.trackId !== id) {
      this.bars = null;
      this.trackId = null;
    }
    if (!track) return;
    try {
      const bars = await api.waveform(track);
      if (this.wanted !== id) return;
      this.bars = bars;
      this.trackId = id;
    } catch (e) {
      // The plain bar stays; a missing helper download is worth saying once.
      if (this.wanted === id) this.wanted = null;
      const code = isAppError(e) ? e.code : "";
      if (!this.warned && (code === "install_failed" || code === "verify_failed" || code === "network")) {
        this.warned = true;
        ui.toast(t("waveform.failed"), "alert");
      }
    }
  }

  clear(): void {
    this.wanted = null;
    this.bars = null;
    this.trackId = null;
  }
}

export const waveform = new WaveformStore();
