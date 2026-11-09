// The player: owns the queue, drives the audio engine, talks to the backend, and keeps the
// Windows media overlay and the saved queue in sync.
import { listen } from "@tauri-apps/api/event";
import { api, artistLine, IMPORT_ONLY, isAppError, type Prepared, type Track } from "../api";
import { AudioEngine } from "../audio/engine";
import { t } from "../i18n/index.svelte";
import * as Q from "../player/queue";
import { dialogs } from "./dialogs.svelte";
import { library } from "./library.svelte";
import { offline } from "./offline.svelte";
import { settings } from "./settings.svelte";
import { ui } from "./ui.svelte";

const SAVE_DELAY_MS = 800;
const POSITION_SAVE_MS = 15_000;
const MEDIA_SYNC_MS = 5_000;
const RESTART_THRESHOLD_MS = 3_000;
const SOURCE_TOAST_COOLDOWN_MS = 60_000;
const AUTOPLAY_BATCH = 10;
const RADIO_BATCH = 25;
/** Imported songs this far ahead get matched before they are reached. */
const PREMATCH_AHEAD = 3;
/** After this many songs in a row fail, stop instead of racing through the whole queue. */
const MAX_FAILS_IN_A_ROW = 3;

interface Saved extends Q.QueueState {
  positionMs: number;
  volume: number;
  muted: boolean;
}

class Player {
  queue = $state<Q.QueueState>(Q.emptyQueue);
  positionMs = $state(0);
  durationMs = $state(0);
  playing = $state(false);
  buffering = $state(false);
  /** True while the current song is being resolved, before audio starts. */
  loading = $state(false);
  volume = $state(0.8);
  muted = $state(false);
  /** The track whose audio actually plays, when it is not the queued one (an imported song's
   * match, or a stand-in from another source). */
  via = $state<Track | null>(null);

  private engine: AudioEngine;
  private loadToken = 0;
  private loadedUid: string | null = null;
  private restoreAtMs = 0;
  private preloadedUid: string | null = null;
  private retriedUid: string | null = null;
  private saveTimer = 0;
  private lastPositionSave = 0;
  private lastMediaSync = 0;
  private sourceToastAt = new Map<string, number>();
  private autoplayBusy = false;
  private failsInARow = 0;

  constructor() {
    this.engine = new AudioEngine({
      time: (pos, dur) => this.onTime(pos, dur),
      advanced: () => this.onAdvanced(),
      ended: () => this.onEnded(),
      state: (playing, buffering) => {
        this.playing = playing;
        this.buffering = buffering;
        this.syncMedia();
      },
      error: (reason) => this.onError(reason),
      nearEnd: () => this.preloadNext(),
    });
  }

  get current(): Q.QueueItem | null {
    return Q.current(this.queue);
  }

  get upcoming(): Q.QueueItem[] {
    return Q.upcoming(this.queue);
  }

  /** Wires backend events. Called once after sign-in. */
  async attach(): Promise<void> {
    await listen<{ action: string; positionMs: number | null }>("media-key", ({ payload }) => {
      if (payload.action === "play") this.play();
      else if (payload.action === "pause") this.pause();
      else if (payload.action === "next") this.next();
      else if (payload.action === "previous") this.previous();
      else if (payload.action === "seek" && payload.positionMs != null) this.seek(payload.positionMs);
    });
    await listen("output-device-changed", () => {
      if (settings.values.pauseOnDeviceChange && this.playing) this.pause();
    });
    await listen<{ source: string; ok: boolean; fallback: string | null }>("source-status", ({ payload }) => {
      if (payload.ok || !payload.fallback) return;
      const last = this.sourceToastAt.get(payload.source) ?? 0;
      if (Date.now() - last < SOURCE_TOAST_COOLDOWN_MS) return;
      this.sourceToastAt.set(payload.source, Date.now());
      ui.toast(t("toast.sourceFailing", { source: sourceLabel(payload.source), other: sourceLabel(payload.fallback) }), "alert", {
        label: t("offline.details"),
        run: () => dialogs.open({ kind: "connection" }),
      });
    });
    await listen("app-closing", async () => {
      await this.saveNow().catch(() => {});
      await api.exitApp();
    });
  }

  applySettings(): void {
    this.engine.crossfadeS = settings.values.crossfade;
    this.engine.setNormalize(settings.values.normalize);
  }

  async restore(): Promise<void> {
    this.applySettings();
    const raw = await api.queueLoad().catch(() => null);
    const r = raw as Partial<Saved> | null;
    if (typeof r?.volume === "number" && r.volume >= 0 && r.volume <= 1) this.volume = r.volume;
    this.muted = r?.muted === true;
    this.engine.setVolume(this.volume);
    this.engine.setMuted(this.muted);
    const q = Q.restore(raw);
    if (!q) return;
    if (settings.values.startup === "empty") {
      this.queue = { ...Q.emptyQueue, shuffle: q.shuffle, repeat: q.repeat };
      return;
    }
    this.queue = q;
    this.restoreAtMs = typeof r?.positionMs === "number" && r.positionMs > 0 ? r.positionMs : 0;
    this.positionMs = this.restoreAtMs;
    this.durationMs = this.current?.track.durationMs ?? 0;
    const cur = this.current;
    if (cur) api.mediaTrack(mediaInfo(cur.track)).catch(() => {});
  }

  /** Stops audio and forgets everything in memory (sign-out). */
  async reset(): Promise<void> {
    await this.engine.pause();
    this.loadToken++;
    this.queue = Q.emptyQueue;
    this.loadedUid = null;
    this.via = null;
    this.positionMs = this.durationMs = 0;
  }

  // ---- queue actions -------------------------------------------------------------------

  /** Replaces the queue. `playlistId` counts a play of that playlist (shown in its stats). */
  playTracks(tracks: Track[], start = 0, playlistId?: string): void {
    this.setQueue(Q.playNow(this.queue, tracks, start));
    this.loadCurrent(true);
    if (playlistId) api.playlistPlayed(playlistId).catch(() => {});
  }

  /** Plays the song, then fills the queue with similar songs. */
  async startRadio(track: Track): Promise<void> {
    this.playTracks([track]);
    try {
      const similar = (await api.autoplayTracks(track)).filter((x) => x.id !== track.id && !library.isHidden(x)).slice(0, RADIO_BATCH);
      if (this.current?.track.id === track.id) this.setQueue(Q.addToQueue(this.queue, similar));
      ui.toast(t("toast.radioStarted", { title: track.title }), "radio");
    } catch {
      ui.toast(t("toast.autoplayFailed"), "alert");
    }
  }

  playNextTracks(tracks: Track[]): void {
    const wasEmpty = !this.current;
    this.setQueue(Q.playNext(this.queue, tracks));
    if (wasEmpty) this.loadCurrent(true);
  }

  addTracks(tracks: Track[]): void {
    const wasEmpty = !this.current;
    this.setQueue(Q.addToQueue(this.queue, tracks));
    if (wasEmpty) this.loadCurrent(true);
  }

  /** What a double-click does, per the user's setting. */
  activate(tracks: Track[], index: number): void {
    if (settings.values.doubleClick === "queue") {
      this.addTracks([tracks[index]]);
      ui.toast(t("toast.addedToQueue"), "queue");
    } else this.playTracks(tracks, index);
  }

  remove(uid: string): void {
    const wasCurrent = this.current?.uid === uid;
    this.setQueue(Q.remove(this.queue, uid));
    if (wasCurrent) this.loadCurrent(this.playing);
  }

  move(from: number, to: number): void {
    this.setQueue(Q.move(this.queue, from, to));
  }

  clearUpcoming(): void {
    this.setQueue(Q.clearUpcoming(this.queue));
  }

  jumpTo(uid: string): void {
    const i = this.queue.items.findIndex((x) => x.uid === uid);
    if (i < 0) return;
    this.setQueue({ ...this.queue, index: i });
    this.loadCurrent(true);
  }

  toggleShuffle(): void {
    this.setQueue(Q.setShuffle(this.queue, !this.queue.shuffle));
  }

  cycleRepeat(): void {
    this.setQueue({ ...this.queue, repeat: Q.cycleRepeat(this.queue.repeat) });
  }

  /** Every queue change goes through here: it saves, and drops a preload that no longer fits. */
  private setQueue(q: Q.QueueState): void {
    this.queue = q;
    const expected = Q.nextIndex(q, true, this.skipItem);
    const expectedUid = expected == null ? null : q.items[expected]?.uid;
    if (this.preloadedUid && this.preloadedUid !== expectedUid) {
      this.engine.clearNext();
      this.preloadedUid = null;
    }
    this.scheduleSave();
  }

  // ---- transport -----------------------------------------------------------------------

  async play(): Promise<void> {
    const cur = this.current;
    if (!cur) return;
    if (this.loadedUid !== cur.uid) return this.loadCurrent(true, this.restoreAtMs);
    await this.engine.play();
  }

  async pause(): Promise<void> {
    await this.engine.pause();
    this.saveNow().catch(() => {});
  }

  toggle(): void {
    if (this.playing || this.loading) this.pause();
    else this.play();
  }

  next(): void {
    const i = Q.nextIndex(this.queue, false);
    if (i == null) return;
    this.setQueue({ ...this.queue, index: i });
    this.loadCurrent(true);
  }

  previous(): void {
    if (this.engine.positionMs > RESTART_THRESHOLD_MS && this.loadedUid) {
      this.seek(0);
      return;
    }
    this.setQueue({ ...this.queue, index: Q.prevIndex(this.queue) });
    this.loadCurrent(true);
  }

  seek(ms: number): void {
    if (!this.current) return;
    const target = Math.max(0, Math.min(ms, this.durationMs || ms));
    this.positionMs = target;
    if (this.loadedUid !== this.current.uid) {
      this.restoreAtMs = target;
      return;
    }
    this.engine.seek(target);
    this.preloadedUid = null;
    this.syncMedia(true);
    this.scheduleSave();
  }

  setVolume(v: number): void {
    this.volume = Math.min(1, Math.max(0, v));
    if (this.volume > 0 && this.muted) this.muted = false;
    this.engine.setMuted(this.muted);
    this.engine.setVolume(this.volume);
    this.scheduleSave();
  }

  toggleMute(): void {
    this.muted = !this.muted;
    this.engine.setMuted(this.muted);
    this.scheduleSave();
  }

  // ---- loading -------------------------------------------------------------------------

  private async prepare(track: Track, retry = false): Promise<Prepared> {
    return api.prepareTrack(track, settings.values.quality, retry);
  }

  /** Resolves and starts the current queue item. Later calls cancel earlier ones. */
  private async loadCurrent(autoplay: boolean, startMs = 0): Promise<void> {
    const item = this.current;
    const token = ++this.loadToken;
    this.restoreAtMs = 0;
    this.preloadedUid = null;
    if (!item) return;
    this.loading = autoplay;
    this.via = null;
    this.positionMs = startMs;
    this.durationMs = item.track.durationMs ?? 0;
    api.mediaTrack(mediaInfo(item.track)).catch(() => {});
    this.warmCover(item.track);
    try {
      const prepared = await this.prepare(item.track);
      if (token !== this.loadToken) return;
      this.applyPrepared(item, prepared);
      this.loadedUid = item.uid;
      this.failsInARow = 0;
      this.retriedUid = null;
      await this.engine.load(prepared.url, { startMs, autoplay, loudnessDb: prepared.loudnessDb });
      this.prefetchFollowing();
    } catch (e) {
      if (token !== this.loadToken) return;
      this.loadedUid = null;
      this.failAndSkip(item, e);
    } finally {
      if (token === this.loadToken) this.loading = false;
    }
  }

  private applyPrepared(item: Q.QueueItem, prepared: Prepared): void {
    this.via = prepared.via;
    if (prepared.standIn) {
      ui.toast(t("toast.standIn", { source: sourceLabel(prepared.standIn.source), title: item.track.title }), "info");
    }
  }

  private failAndSkip(item: Q.QueueItem, e: unknown): void {
    console.error("playback failed", item.track.id, e);
    const code = isAppError(e) ? e.code : "";
    const text =
      code === "offline" || code === "network"
        ? t("error.offline")
        : code === "nothing_playable"
          ? t("toast.nothingPlayable", { title: item.track.title })
          : t("toast.skipped", { title: item.track.title });
    this.playing = false;
    if (code === "offline" && !offline.online) {
      // Offline mode: go on with the next song that is on this PC. The offline toast
      // already explained why songs are passed over.
      const i = Q.nextIndex(this.queue, true, this.skipItem);
      if (i != null && i !== this.queue.index) {
        this.setQueue({ ...this.queue, index: i });
        this.loadCurrent(true);
      } else ui.toast(t("offline.nothingLeft"), "alert");
      return;
    }
    ui.toast(text, "alert");
    this.failsInARow += 1;
    if (code === "offline" || code === "network" || this.failsInARow >= MAX_FAILS_IN_A_ROW) {
      this.failsInARow = 0;
      return;
    }
    const i = Q.nextIndex(this.queue, false);
    // Skip forward unless that would wrap around to the song that just failed.
    if (i != null && i !== this.queue.index && !(this.queue.repeat === "all" && i === 0)) {
      this.setQueue({ ...this.queue, index: i });
      this.loadCurrent(true);
    }
  }

  /** Tells the backend to start downloading the next song now, so it's ready in time. */
  private prefetchFollowing(): void {
    const i = Q.nextIndex(this.queue, true, this.skipItem);
    const next = i == null ? null : this.queue.items[i];
    if (next && next.uid !== this.current?.uid) api.prefetchTrack(next.track, settings.values.quality).catch(() => {});
    const imported = this.upcoming.slice(0, PREMATCH_AHEAD).map((x) => x.track).filter((x) => IMPORT_ONLY.has(x.source) && !x.via);
    if (imported.length) api.prematch(imported).catch(() => {});
  }

  /** Passed over when the queue moves on by itself: hidden songs, and while offline, songs
   * that aren't on this PC. */
  private skipItem = (item: Q.QueueItem): boolean => library.isHidden(item.track) || (!offline.online && !offline.isAvailable(item.track));

  /** Looks up a sharper cover in the background, so it is cached when a big view needs it. */
  private warmCover(track: Track): void {
    if (offline.online) api.coverHd(track).catch(() => {});
  }

  private async preloadNext(): Promise<void> {
    let i = Q.nextIndex(this.queue, true, this.skipItem);
    if (i == null && settings.values.autoplay && this.queue.repeat === "off" && offline.online) {
      await this.extendWithAutoplay();
      i = Q.nextIndex(this.queue, true, this.skipItem);
    }
    if (i == null) return;
    const item = this.queue.items[i];
    const token = this.loadToken;
    try {
      const prepared = await this.prepare(item.track);
      // The queue may have changed while we waited; only preload if this is still next.
      if (token !== this.loadToken || Q.nextIndex(this.queue, true, this.skipItem) !== this.queue.items.indexOf(item)) return;
      this.engine.preloadNext(prepared.url, prepared.loudnessDb);
      this.preloadedUid = item.uid;
    } catch {
      // Resolved again (with a visible error) when the song actually starts.
    }
  }

  private async extendWithAutoplay(): Promise<boolean> {
    const seed = this.current?.track;
    if (!seed || this.autoplayBusy) return false;
    this.autoplayBusy = true;
    try {
      const known = new Set(this.queue.items.map((i) => i.track.id));
      const found = (await api.autoplayTracks(seed)).filter((t) => !known.has(t.id) && !library.isHidden(t)).slice(0, AUTOPLAY_BATCH);
      if (found.length === 0) return false;
      this.setQueue(Q.addToQueue(this.queue, found));
      return true;
    } catch {
      ui.toast(t("toast.autoplayFailed"), "alert");
      return false;
    } finally {
      this.autoplayBusy = false;
    }
  }

  // ---- engine events -------------------------------------------------------------------

  private onTime(pos: number, dur: number): void {
    this.positionMs = pos;
    if (dur > 0) this.durationMs = dur;
    const now = Date.now();
    if (this.playing && now - this.lastPositionSave > POSITION_SAVE_MS) {
      this.lastPositionSave = now;
      this.saveNow().catch(() => {});
    }
    if (this.playing && now - this.lastMediaSync > MEDIA_SYNC_MS) this.syncMedia();
  }

  private onAdvanced(): void {
    const index = this.queue.items.findIndex((i) => i.uid === this.preloadedUid);
    // Cleared before setQueue, which would otherwise treat the preload as stale and cut the fade.
    this.preloadedUid = null;
    const item = this.queue.items[index];
    if (!item) return;
    this.setQueue({ ...this.queue, index });
    this.loadedUid = item.uid;
    this.loadToken++;
    this.via = null;
    this.positionMs = 0;
    this.durationMs = item.track.durationMs ?? 0;
    this.retriedUid = null;
    api.mediaTrack(mediaInfo(item.track)).catch(() => {});
    this.warmCover(item.track);
    this.prefetchFollowing();
  }

  private async onEnded(): Promise<void> {
    let i = Q.nextIndex(this.queue, true, this.skipItem);
    if (i == null && settings.values.autoplay && this.queue.repeat === "off" && offline.online && (await this.extendWithAutoplay())) {
      i = Q.nextIndex(this.queue, true, this.skipItem);
    }
    if (i == null) {
      this.playing = false;
      this.syncMedia(true);
      return;
    }
    this.setQueue({ ...this.queue, index: i });
    this.loadCurrent(true);
  }

  /** A stream broke mid-song: resolve it again once and continue from the same spot. */
  private async onError(reason: "load" | "stall"): Promise<void> {
    const item = this.current;
    if (!item) return;
    if (this.retriedUid === item.uid) {
      // Unload so the dead stream can't fire more errors and start another retry.
      this.engine.unload();
      this.loadedUid = null;
      this.failAndSkip(item, { code: reason, detail: "stream failed twice" });
      return;
    }
    this.retriedUid = item.uid;
    const at = this.engine.positionMs;
    const token = ++this.loadToken;
    this.loading = true;
    try {
      const prepared = await this.prepare(item.track, true);
      if (token !== this.loadToken) return;
      await this.engine.load(`${prepared.url}?r=${Date.now()}`, { startMs: at, autoplay: true, loudnessDb: prepared.loudnessDb });
    } catch (e) {
      if (token === this.loadToken) this.failAndSkip(item, e);
    } finally {
      if (token === this.loadToken) this.loading = false;
    }
  }

  // ---- persistence & OS ----------------------------------------------------------------

  private syncMedia(force = false): void {
    const now = Date.now();
    if (!force && now - this.lastMediaSync < 300) return;
    this.lastMediaSync = now;
    if (!this.current) return;
    api.mediaState(this.playing, this.positionMs, this.durationMs).catch(() => {});
  }

  private scheduleSave(): void {
    clearTimeout(this.saveTimer);
    this.saveTimer = window.setTimeout(() => this.saveNow().catch(() => {}), SAVE_DELAY_MS);
  }

  async saveNow(): Promise<void> {
    clearTimeout(this.saveTimer);
    const saved: Saved = { ...this.queue, positionMs: Math.round(this.positionMs), volume: this.volume, muted: this.muted };
    await api.queueSave($state.snapshot(saved));
  }
}

function mediaInfo(track: Track) {
  return { title: track.title, artist: artistLine(track), album: track.album?.name ?? null, cover: track.cover, durationMs: track.durationMs ?? 0 };
}

export function sourceLabel(source: string): string {
  return source === "soundcloud" ? t("source.soundcloud") : source === "local" ? t("source.local") : t("source.youtube");
}

export const player = new Player();
