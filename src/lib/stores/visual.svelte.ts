// Visual quality as it applies right now: the setting (or this PC's recommendation), dropped
// to Low while Game mode is active. Views read `quality`; the page root carries it as
// data-quality for CSS.
import type { HardwareInfo, VisualQuality } from "../api";
import { settings, type LyricsStyle } from "./settings.svelte";

class VisualStore {
  /** A fullscreen game is in front and Game mode is on. */
  game = $state(false);
  hardware = $state<HardwareInfo | null>(null);

  get recommended(): VisualQuality {
    return this.hardware?.recommended.quality ?? "high";
  }

  /** What the user chose, with "auto" resolved. */
  get chosen(): VisualQuality {
    const q = settings.values.visualQuality;
    return q === "auto" ? this.recommended : q;
  }

  get quality(): VisualQuality {
    return this.game ? "low" : this.chosen;
  }

  /** Low quality keeps lyrics plain: whole lines fade, no word effects. */
  get lyricsStyle(): LyricsStyle {
    return this.quality === "low" ? "fade" : settings.values.lyricsStyle;
  }
}

export const visual = new VisualStore();
