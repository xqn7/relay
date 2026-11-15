// The main window's side of desktop integration: feeds the mini player and desktop lyrics,
// acts on their buttons, keeps the tray and taskbar texts in the UI's language, follows
// Game mode, offers downloaded updates, and suggests a lower visual quality when animations
// stutter.
import { emit, listen } from "@tauri-apps/api/event";
import { untrack } from "svelte";
import { api, artistLine, proxiedCover, type DesktopInfo, type UpdateInfo, type VisualQuality } from "../api";
import { LYRICS_EVENT, NOW_EVENT, READY_EVENT, REMOTE_EVENT, positionNow, type LyricsFeed, type NowFeed, type RemoteAction } from "../desktop/feed";
import { DEV_BUILD } from "../brand";
import { i18n, t } from "../i18n/index.svelte";
import { buildLines } from "../lyrics/lines";
import { backgrounds } from "./backgrounds.svelte";
import { dialogs } from "./dialogs.svelte";
import { library } from "./library.svelte";
import { lyrics } from "./lyrics.svelte";
import { player } from "./player.svelte";
import { settings } from "./settings.svelte";
import { ui } from "./ui.svelte";
import { visual } from "./visual.svelte";

/** A position report this far off from where the overlays think the song is means a seek. */
const SEEK_DRIFT_MS = 1500;
/** The version that last ran on this PC, to notice an update. */
const SEEN_VERSION_KEY = "relay.seenVersion";
/** Desktop lyrics with nothing to show for this long close their window. */
const LYRICS_IDLE_MS = 60_000;
/** How long frame times are sampled when the fullscreen player opens. */
const FRAME_SAMPLE_MS = 5000;
/** Share of slow frames that counts as stuttering. */
const SLOW_SHARE = 0.25;

class DesktopStore {
  info = $state<DesktopInfo | null>(null);
  update = $state<UpdateInfo | null>(null);
  private lyricsOpen = false;
  private lyricsIdleTimer = 0;
  lyricsUnlocked = $state(false);
  private attached = false;
  private lastNow: NowFeed | null = null;
  private qualityHintShown = false;
  private blockedShown = false;

  /** Wires events and effects once, after sign-in. */
  async attach(): Promise<void> {
    if (this.attached) return;
    this.attached = true;
    await Promise.all([
      listen<boolean>("game-mode", ({ payload }) => (visual.game = payload === true)),
      listen<UpdateInfo>("update-ready", ({ payload }) => this.offerUpdate(payload)),
      listen<boolean>("lyrics-unlocked", ({ payload }) => (this.lyricsUnlocked = payload === true)),
      listen("helper-blocked", () => this.warnBlocked()),
      listen(READY_EVENT, () => {
        this.sendNow();
        this.sendLyrics();
      }),
      listen<RemoteAction>(REMOTE_EVENT, ({ payload }) => this.remote(payload)),
    ]);
    // The update check runs right at start, often before sign-in; one it already found is
    // offered now.
    api
      .updateReady()
      .then((info) => info && !this.update && this.offerUpdate(info))
      .catch(() => {});
    this.info = await api.desktopInfo().catch(() => null);
    if (this.info?.helperBlocked) this.warnBlocked();
    if (this.info) this.showChangelog(this.info.version);
    visual.game = this.info?.game ?? false;
    this.lyricsUnlocked = this.info?.lyricsUnlocked ?? false;
    api
      .hardwareInfo()
      .then((h) => {
        visual.hardware = h;
      })
      .catch((e) => console.error("hardware info failed", e));
    $effect.root(() => this.effects());
  }

  /** Started with Windows into the tray, with "carry on playing" turned on. */
  autoPlay(): void {
    if (this.info?.startedInTray && settings.values.startPlay && player.current) player.play();
  }

  private effects(): void {
    $effect(() => {
      const root = document.documentElement;
      root.dataset.quality = visual.quality;
      root.toggleAttribute("data-game", visual.game);
    });

    // Song, play state, like and look for the overlays.
    $effect(() => {
      void [player.current, player.playing, settings.values.speed, visual.quality, backgrounds.activeIn("mini")];
      const track = player.current?.track;
      if (track) void library.isLiked(track);
      untrack(() => this.sendNow());
    });

    // A jump in position (seek, loop, skip within a song) is passed on; normal progress isn't.
    $effect(() => {
      const pos = player.positionMs;
      untrack(() => {
        if (this.lastNow && Math.abs(positionNow(this.lastNow) - pos) > SEEK_DRIFT_MS) this.sendNow();
      });
    });

    $effect(() => {
      void [settings.values.desktopLyrics, lyrics.status, lyrics.data, player.durationMs, visual.lyricsStyle, this.lyricsUnlocked];
      untrack(() => this.sendLyrics());
    });

    // The desktop lyrics window only exists while it has something to show.
    $effect(() => {
      const wanted = settings.values.desktopLyrics && player.playing && lyrics.status === "ready" && !!lyrics.data?.synced;
      untrack(() => this.setLyricsWanted(wanted));
    });

    // Tray, taskbar and Discord texts follow the UI language.
    $effect(() => {
      void i18n.lang;
      untrack(() => api.uiLabels(labels()).catch((e) => console.error("tray labels failed", e)));
    });

    $effect(() => {
      if (ui.fullscreenOpen && visual.quality !== "low" && !this.qualityHintShown) untrack(() => this.watchFrames());
    });
  }

  private sendNow(): void {
    const item = player.current;
    const image = backgrounds.activeIn("mini");
    const feed: NowFeed = {
      track: item
        ? { id: item.track.id, title: item.track.title, artist: artistLine(item.track), cover: item.track.cover ? proxiedCover(item.track.cover) : null }
        : null,
      liked: item ? library.isLiked(item.track) : false,
      playing: player.playing,
      posMs: player.livePositionMs(),
      at: Date.now(),
      rate: settings.values.speed,
      quality: visual.quality,
      background: image ? { url: image.url, look: backgrounds.look(image) } : null,
    };
    this.lastNow = feed;
    emit(NOW_EVENT, feed).catch(() => {});
  }

  private sendLyrics(): void {
    if (!settings.values.desktopLyrics) return;
    const synced = lyrics.status === "ready" ? lyrics.data?.synced : null;
    const feed: LyricsFeed = {
      trackId: lyrics.track?.id ?? null,
      lines: synced ? buildLines(synced, player.durationMs) : [],
      style: visual.lyricsStyle,
      unlocked: this.lyricsUnlocked,
    };
    emit(LYRICS_EVENT, feed).catch(() => {});
  }

  private remote(action: RemoteAction): void {
    if (action === "toggle") player.toggle();
    else if (action === "next") player.next();
    else if (action === "previous") player.previous();
    else if (action === "like" && player.current) library.toggleLike(player.current.track);
  }

  /** Opens the lyrics window at once when wanted; closes it after a quiet minute (a pause or a
   * song without lyrics), so short breaks don't rebuild it. */
  private setLyricsWanted(wanted: boolean): void {
    clearTimeout(this.lyricsIdleTimer);
    if (wanted) {
      if (this.lyricsOpen) return;
      this.lyricsOpen = true;
      // A failed open is tried again with the next song or resume.
      api.lyricsActive(true).catch((e) => {
        this.lyricsOpen = false;
        console.error("desktop lyrics failed", e);
      });
      return;
    }
    if (!this.lyricsOpen) return;
    this.lyricsIdleTimer = window.setTimeout(() => {
      this.lyricsOpen = false;
      api.lyricsActive(false).catch((e) => console.error("desktop lyrics failed", e));
    }, LYRICS_IDLE_MS);
  }

  /** Once per start, whether the event came first or sign-in did. */
  private warnBlocked(): void {
    if (this.blockedShown) return;
    this.blockedShown = true;
    ui.toast(t("helper.blocked"), "alert");
  }

  setLyricsUnlocked(on: boolean): void {
    api.lyricsUnlock(on).catch((e) => console.error("desktop lyrics lock failed", e));
  }

  // ---- updates -----------------------------------------------------------------------------

  /** After an update (not a first install, not Relay Dev): that release's notes, once. */
  private showChangelog(version: string): void {
    let seen: string | null = null;
    try {
      seen = localStorage.getItem(SEEN_VERSION_KEY);
      localStorage.setItem(SEEN_VERSION_KEY, version);
    } catch {
      return;
    }
    if (DEV_BUILD || !seen || seen === version) return;
    api
      .releaseNotes()
      .then((notes) => notes.trim() && dialogs.open({ kind: "changelog", version, notes }))
      .catch((e) => console.error("release notes unavailable", e));
  }

  private offerUpdate(info: UpdateInfo): void {
    this.update = info;
    ui.toast(t("update.ready", { version: info.version }), "download", { label: t("update.details"), run: () => dialogs.open({ kind: "update", info }) });
  }

  async checkForUpdate(): Promise<UpdateInfo | null> {
    const found = await api.updateCheck();
    if (found) this.update = found;
    return found;
  }

  async install(): Promise<void> {
    await player.saveNow().catch(() => {});
    await api.updateInstall();
  }

  // ---- dropped frames ----------------------------------------------------------------------

  /** Samples frame times for a few seconds; if many frames are late, suggests the next lower
   * quality once per run instead of switching on its own. */
  private watchFrames(): void {
    const times: number[] = [];
    const start = performance.now();
    const tick = (now: number) => {
      times.push(now);
      if (now - start < FRAME_SAMPLE_MS && ui.fullscreenOpen) requestAnimationFrame(tick);
      else this.judgeFrames(times);
    };
    requestAnimationFrame(tick);
  }

  private judgeFrames(times: number[]): void {
    if (times.length < 30 || this.qualityHintShown || document.visibilityState !== "visible") return;
    const gaps = times.slice(1).map((x, i) => x - times[i]);
    const sorted = [...gaps].sort((a, b) => a - b);
    const typical = sorted[Math.floor(sorted.length / 2)];
    const slow = gaps.filter((g) => g > Math.max(typical * 1.8, 34)).length / gaps.length;
    if (slow < SLOW_SHARE) return;
    const lower: VisualQuality | null = visual.chosen === "high" ? "medium" : visual.chosen === "medium" ? "low" : null;
    if (!lower) return;
    this.qualityHintShown = true;
    ui.toast(t("quality.stutter", { level: t(`quality.${lower}`) }), "info", {
      label: t("quality.use", { level: t(`quality.${lower}`) }),
      run: () => settings.set("visualQuality", lower),
    });
  }
}

function labels() {
  return {
    play: t("player.play"),
    pause: t("player.pause"),
    next: t("player.next"),
    previous: t("player.previous"),
    privateSession: t("tray.private"),
    miniPlayer: t("tray.miniPlayer"),
    unlockLyrics: t("tray.unlockLyrics"),
    lockLyrics: t("tray.lockLyrics"),
    show: t("tray.show"),
    quit: t("tray.quit"),
    paused: t("discord.pausedMark"),
    listenYoutube: t("discord.listenYoutube"),
    listenSoundcloud: t("discord.listenSoundcloud"),
    listenSpotify: t("discord.listenSpotify"),
  };
}

export const desktop = new DesktopStore();
