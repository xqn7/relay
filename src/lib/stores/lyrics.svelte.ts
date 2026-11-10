// Lyrics of the current song (and its translation), shared by the fullscreen player and the
// idle screen. Looked up when the song changes; the next songs' lyrics are fetched ahead.
import { api, isAppError, type Lyrics, type Track } from "../api";
import { t } from "../i18n/index.svelte";
import { offline } from "./offline.svelte";
import { player } from "./player.svelte";
import { settings } from "./settings.svelte";
import { ui } from "./ui.svelte";

export type LyricsStatus = "idle" | "loading" | "ready" | "none" | "offline" | "error";

/** Songs ahead in the queue whose lyrics are fetched early. */
const PRELOAD_AHEAD = 3;

class LyricsStore {
  track = $state<Track | null>(null);
  status = $state<LyricsStatus>("idle");
  data = $state<Lyrics | null>(null);
  /** One entry per lyrics line, or null when not shown. */
  translation = $state<string[] | null>(null);
  translating = $state(false);
  /** The lyrics are already in the language they'd be translated to. */
  sameLanguage = $state(false);
  private seq = 0;

  /** Called whenever the current song (or the connection) changes. */
  follow(track: Track | null): void {
    if (track?.id === this.track?.id) {
      // Lyrics that couldn't load offline load once the connection is back.
      if (track && this.status === "offline" && offline.online) this.load(track);
      return;
    }
    this.track = track;
    if (track) this.load(track);
    else this.clear();
  }

  private clear(): void {
    this.seq++;
    this.status = "idle";
    this.data = null;
    this.translation = null;
    this.sameLanguage = false;
  }

  async load(track: Track): Promise<void> {
    const id = ++this.seq;
    this.status = "loading";
    this.data = null;
    this.translation = null;
    this.sameLanguage = false;
    try {
      const found = await api.lyricsGet(track);
      if (id !== this.seq) return;
      this.data = found;
      this.status = found ? "ready" : "none";
      if (found && settings.values.translateLyrics) this.translate();
    } catch (e) {
      if (id !== this.seq) return;
      this.status = isAppError(e) && e.code === "offline" ? "offline" : "error";
    }
    if (id === this.seq) this.preloadNext();
  }

  private preloadNext(): void {
    if (!offline.online) return;
    const next = player.upcoming.slice(0, PRELOAD_AHEAD).map((x) => x.track);
    if (next.length) api.lyricsPreload(next, settings.values.translateLyrics ? settings.values.translateTo : null).catch(() => {});
  }

  /** Replaces the lyrics after the editor saved or reset them. */
  apply(found: Lyrics | null): void {
    this.seq++;
    this.data = found;
    this.status = found ? "ready" : "none";
    this.translation = null;
    this.sameLanguage = false;
    if (found && settings.values.translateLyrics) this.translate();
  }

  /** `asked`: the Translate button was just pressed, so say so when there's nothing to do. */
  async translate(asked = false): Promise<void> {
    const track = this.track;
    if (!track || !this.data) return;
    const id = this.seq;
    const lang = settings.values.translateTo;
    this.translating = true;
    try {
      const result = await api.lyricsTranslate(track.id, lang);
      if (id !== this.seq) return;
      this.sameLanguage = result.sourceLang?.toLowerCase().startsWith(lang) ?? false;
      this.translation = this.sameLanguage ? null : result.lines;
      // The note at the top of the lyrics is out of view once the song is underway.
      if (asked && this.sameLanguage) ui.toast(t("lyrics.sameLanguage"), "info");
    } catch (e) {
      if (id !== this.seq) return;
      const code = isAppError(e) ? e.code : "";
      ui.toast(
        code === "deepl_key_invalid"
          ? t("lyrics.deeplInvalid")
          : code === "deepl_quota"
            ? t("lyrics.deeplQuota")
            : code === "offline"
              ? t("lyrics.translateOffline")
              : t("lyrics.translateFailed"),
        "alert",
      );
    } finally {
      if (id === this.seq) this.translating = false;
    }
  }

  /** The Translate button: remembered, so the next songs come translated too. */
  setTranslate(on: boolean): void {
    settings.set("translateLyrics", on);
    if (on) this.translate(true);
    else {
      this.translation = null;
      this.sameLanguage = false;
    }
  }
}

export const lyrics = new LyricsStore();
