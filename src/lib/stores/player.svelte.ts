// The player: owns the queue, drives the audio engine, talks to the backend, and keeps the
// Windows media overlay and the saved queue in sync.
import { listen } from "@tauri-apps/api/event";
import { api, artistLine, formatTime, IMPORT_ONLY, isAppError, type NowPlayingInfo, type Prepared, type Track, type Trim } from "../api";
import { AudioEngine, type Jump } from "../audio/engine";
import { t } from "../i18n/index.svelte";
import { PlayCounter } from "../player/playCount";
import * as Q from "../player/queue";
import { dialogs } from "./dialogs.svelte";
import { history } from "./history.svelte";
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
/** Radio refills when fewer songs than this are left. */
const RADIO_REFILL_AT = 5;
/** Imported songs this far ahead get matched before they are reached. */
const PREMATCH_AHEAD = 3;
/** After this many songs in a row fail, stop instead of racing through the whole queue. */
const MAX_FAILS_IN_A_ROW = 3;
/** A–B markers start this far apart when first shown. */
const DEFAULT_RANGE_MS = 30_000;
const MIN_RANGE_MS = 1_000;

/** Two markers on the progress bar, for the A–B loop, trims and clips. */
export interface Markers {
  aMs: number;
  bMs: number;
}

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
  /** The A–B markers on the progress bar, while they're shown. */
  markers = $state<Markers | null>(null);
  looping = $state(false);
  /** Non-music sections SponsorBlock found in the playing video (seconds). */
  skipSections = $state<[number, number][]>([]);
  /** Called after the volume changed (the output device remembers it). */
  onVolumeChange: (() => void) | null = null;

  readonly engine: AudioEngine;
  /** A clip preview stops here. */
  private previewUntilMs: number | null = null;
  private preloadedVia: Track | null = null;
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
  /** Counts the current playback as a play once it's been listened to long enough. */
  private counter = new PlayCounter();
  /** The recorded play of the current playback, updated with how long it was listened to. */
  private playId: string | null = null;

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

  /** The song that plays when this one ends, if any (not the same one on repeat). */
  get nextItem(): Q.QueueItem | null {
    const i = Q.nextIndex(this.queue, true, this.skipItem);
    const item = i == null ? null : this.queue.items[i];
    return item && item.uid !== this.current?.uid ? item : null;
  }

  /** The exact playback position right now, for views that move with the music (lyrics).
   * `positionMs` only updates a few times a second. */
  livePositionMs(): number {
    return this.loadedUid && this.loadedUid === this.current?.uid ? this.engine.positionMs : this.positionMs;
  }

  /** Wires backend events. Called once after sign-in. */
  async attach(): Promise<void> {
    await listen<{ action: string; positionMs: number | null }>("media-key", ({ payload }) => {
      if (payload.action === "play") this.play();
      else if (payload.action === "pause") this.pause();
      else if (payload.action === "toggle") this.toggle();
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

  /** Plays the song, then keeps the queue filled with similar songs (radio mode). */
  async startRadio(track: Track): Promise<void> {
    this.playTracks([track]);
    this.setQueue(Q.startRadio(this.queue, track.title));
    const uid = this.current?.uid;
    if (await this.refillRadio()) {
      if (this.current?.uid === uid) ui.toast(t("toast.radioStarted", { title: track.title }), "radio");
    } else ui.toast(t("toast.autoplayFailed"), "alert");
  }

  /** Adds similar songs to the current song's radio. False when none could be found. */
  private async refillRadio(): Promise<boolean> {
    const seed = this.current?.track;
    if (!seed || !this.queue.radio || this.autoplayBusy) return false;
    this.autoplayBusy = true;
    try {
      const known = new Set(this.queue.items.map((i) => i.track.id));
      const found = (await api.autoplayTracks(seed)).filter((x) => !known.has(x.id) && !library.isHidden(x)).slice(0, RADIO_BATCH);
      // The user may have started something else meanwhile.
      if (!this.queue.radio || this.current?.track.id !== seed.id || found.length === 0) return found.length > 0;
      this.setQueue(Q.addToQueue(this.queue, found));
      return true;
    } catch {
      return false;
    } finally {
      this.autoplayBusy = false;
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
    this.setQueue({ ...this.queue, repeat: Q.cycleRepeat(this.queue.repeat, !!this.queue.radio) });
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

  // ---- play counts ---------------------------------------------------------------------

  /** A new playback of `item` starts: the previous one's listening time is saved first. */
  private beginCount(item: Q.QueueItem): void {
    this.flushListened();
    this.playId = null;
    this.counter.start(item.uid);
  }

  private async recordPlay(item: Q.QueueItem): Promise<void> {
    try {
      const id = await history.record(item.track, this.counter.listenedMs);
      if (id && this.counter.key === item.uid) this.playId = id;
    } catch (e) {
      console.error("recording the play failed", e);
    }
  }

  private flushListened(): void {
    if (this.playId) api.playListened(this.playId, this.counter.listenedMs).catch(() => {});
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
    this.onVolumeChange?.();
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
    this.beginCount(item);
    this.resetSongTools();
    const trim = trimOf(item.track);
    if (startMs === 0 && trim) startMs = trim.startMs;
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
      await this.engine.load(prepared.url, { startMs, endMs: trim?.endMs ?? null, autoplay, loudnessDb: prepared.loudnessDb });
      this.loadSkipSections(item, prepared.via);
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
    if (this.queue.radio && this.upcoming.length < RADIO_REFILL_AT && offline.online) this.refillRadio();
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
    if (i == null && (await this.extendQueue())) {
      i = Q.nextIndex(this.queue, true, this.skipItem);
    }
    if (i == null) return;
    const item = this.queue.items[i];
    const token = this.loadToken;
    try {
      const prepared = await this.prepare(item.track);
      // The queue may have changed while we waited; only preload if this is still next.
      if (token !== this.loadToken || Q.nextIndex(this.queue, true, this.skipItem) !== this.queue.items.indexOf(item)) return;
      const trim = trimOf(item.track);
      this.engine.preloadNext(prepared.url, prepared.loudnessDb, trim?.startMs ?? 0, trim?.endMs ?? null);
      this.preloadedUid = item.uid;
      this.preloadedVia = prepared.via;
    } catch {
      // Resolved again (with a visible error) when the song actually starts.
    }
  }

  /** The queue ran out: radio refills itself, otherwise autoplay adds similar songs if on. */
  private async extendQueue(): Promise<boolean> {
    if (!offline.online) return false;
    if (this.queue.radio) return this.refillRadio();
    return settings.values.autoplay && this.queue.repeat === "off" && this.extendWithAutoplay();
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
    // While the next song is still loading, the old one keeps reporting its time; showing it
    // under the new title would look like the new song started in the middle.
    if (this.loadedUid !== this.current?.uid) return;
    this.positionMs = pos;
    if (dur > 0) this.durationMs = dur;
    const item = this.current;
    if (item && this.playing && this.counter.tick(item.uid, pos, this.durationMs)) this.recordPlay(item);
    if (this.previewUntilMs != null && this.playing && pos >= this.previewUntilMs) {
      this.previewUntilMs = null;
      this.pause();
    }
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
    this.via = this.preloadedVia;
    this.beginCount(item);
    this.resetSongTools();
    this.loadSkipSections(item, this.preloadedVia);
    this.positionMs = trimOf(item.track)?.startMs ?? 0;
    this.durationMs = item.track.durationMs ?? 0;
    this.retriedUid = null;
    api.mediaTrack(mediaInfo(item.track)).catch(() => {});
    this.warmCover(item.track);
    this.prefetchFollowing();
  }

  private async onEnded(): Promise<void> {
    let i = Q.nextIndex(this.queue, true, this.skipItem);
    if (i == null && (await this.extendQueue())) {
      i = Q.nextIndex(this.queue, true, this.skipItem);
    }
    if (i == null) {
      this.playing = false;
      // Play again starts the song over, at its trimmed start.
      const trim = this.current && trimOf(this.current.track);
      if (trim) {
        this.loadedUid = null;
        this.restoreAtMs = trim.startMs;
      }
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

  // ---- A–B markers, loop, trim, skipped sections -------------------------------------------

  /** Forgets the per-song tools when another song starts. */
  private resetSongTools(): void {
    this.markers = null;
    this.looping = false;
    this.previewUntilMs = null;
    this.skipSections = [];
    this.engine.setJumps([]);
  }

  /** Shows the markers: over the trim if the song has one, else from the current position. */
  showMarkers(): void {
    const dur = this.durationMs;
    if (!this.current || dur <= 0 || this.markers) return;
    const trim = trimOf(this.current.track);
    if (trim) {
      this.markers = { aMs: trim.startMs, bMs: trim.endMs ?? dur };
      return;
    }
    const a = Math.min(this.positionMs, Math.max(0, dur - DEFAULT_RANGE_MS));
    this.markers = { aMs: Math.round(a), bMs: Math.round(Math.min(dur, a + DEFAULT_RANGE_MS)) };
  }

  hideMarkers(): void {
    this.markers = null;
    this.looping = false;
    this.updateJumps();
  }

  /** Moves the markers, keeping them inside the song and at least a second apart. */
  setMarkers(aMs: number, bMs: number): void {
    const dur = this.durationMs;
    if (dur <= 0) return;
    let a = Math.max(0, Math.min(aMs, dur - MIN_RANGE_MS));
    let b = Math.max(MIN_RANGE_MS, Math.min(bMs, dur));
    if (b - a < MIN_RANGE_MS) {
      if (aMs !== this.markers?.aMs) a = b - MIN_RANGE_MS;
      else b = a + MIN_RANGE_MS;
    }
    this.markers = { aMs: Math.round(a), bMs: Math.round(b) };
    this.updateJumps();
  }

  toggleLoop(): void {
    if (!this.markers) return;
    this.looping = !this.looping;
    this.updateJumps();
    const { aMs, bMs } = this.markers;
    if (this.looping && (this.positionMs < aMs || this.positionMs >= bMs)) this.seek(aMs);
  }

  /** Plays the marked part once, then pauses (clip preview). */
  async previewMarked(): Promise<void> {
    if (!this.markers) return;
    this.looping = false;
    this.updateJumps();
    this.seek(this.markers.aMs);
    this.previewUntilMs = this.markers.bMs;
    if (!this.playing) await this.play();
  }

  /** Save as trim: the song always plays from A to B from now on. */
  async saveTrim(): Promise<void> {
    const cur = this.current;
    if (!cur || !this.markers) return;
    const { aMs, bMs } = this.markers;
    // Markers at the very ends mean no trim on that side.
    const trim: Trim = { startMs: aMs < 500 ? 0 : aMs, endMs: bMs > this.durationMs - 500 ? null : bMs };
    if (trim.startMs === 0 && trim.endMs == null) return this.resetTrim(cur.track);
    if (!(await library.setTrim(cur.track, trim))) return;
    this.engine.setEnd(trim.endMs);
    this.hideMarkers();
    ui.toast(t("trim.saved", { title: cur.track.title, from: formatTime(aMs), to: formatTime(trim.endMs ?? this.durationMs) }), "scissors", { label: t("toast.undo"), run: () => this.resetTrim(cur.track) });
  }

  async resetTrim(track: Track): Promise<void> {
    if (!(await library.setTrim(track, null))) return;
    if (this.current?.track.id === track.id) this.engine.setEnd(null);
  }

  private updateJumps(): void {
    const jumps: Jump[] = [];
    if (this.looping && this.markers) jumps.push({ fromS: this.markers.bMs / 1000, toS: this.markers.aMs / 1000 });
    if (settings.values.sponsorBlock) for (const [a, b] of this.skipSections) jumps.push({ fromS: a, toS: b });
    this.engine.setJumps(jumps);
  }

  /** SponsorBlock's non-music sections (talking, skits) of a YouTube music video. */
  private async loadSkipSections(item: Q.QueueItem, via: Track | null): Promise<void> {
    const played = via ?? item.track;
    if (!settings.values.sponsorBlock || played.source !== "youtube" || !offline.online) return;
    try {
      const found = await api.sponsorSegments(played.sourceId);
      if (this.current?.uid !== item.uid || found.length === 0) return;
      this.skipSections = found;
      this.updateJumps();
      // An intro right at the start is skipped straight away.
      const first = found[0];
      const pos = this.engine.positionMs / 1000;
      if (first[0] < 1 && pos < first[1] && pos < 3) this.seek(first[1] * 1000);
    } catch {
      // Without SponsorBlock the song simply plays in full.
    }
  }

  /** Turning SponsorBlock on or off applies to the playing song right away. */
  applySkipSetting(): void {
    const cur = this.current;
    if (settings.values.sponsorBlock && cur && this.skipSections.length === 0) this.loadSkipSections(cur, this.via);
    else this.updateJumps();
  }

  /** Plays another version of a song: in its place when it is the one playing, else right away. */
  switchVersion(original: Track, version: Track): void {
    const cur = this.current;
    if (cur && cur.track.id === original.id) {
      this.setQueue({ ...this.queue, items: this.queue.items.map((it) => (it.uid === cur.uid ? { ...it, track: version } : it)) });
      this.loadCurrent(true);
    } else {
      this.playNextTracks([version]);
      if (cur) this.next();
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
    this.flushListened();
    const saved: Saved = { ...this.queue, positionMs: Math.round(this.positionMs), volume: this.volume, muted: this.muted };
    await api.queueSave($state.snapshot(saved));
  }
}

function trimOf(track: Track): Trim | null {
  return library.trims[track.id] ?? null;
}

function mediaInfo(track: Track): NowPlayingInfo {
  const link = (source: string) => (track.source === source ? track.link : null);
  return {
    title: track.title,
    artist: artistLine(track),
    album: track.album?.name ?? null,
    cover: track.cover,
    durationMs: track.durationMs ?? 0,
    // Artist ids of YouTube songs are YouTube Music ids, which is where the photo comes from.
    artistId: track.source === "youtube" ? (track.artists[0]?.id ?? null) : null,
    links: { youtube: link("youtube"), soundcloud: link("soundcloud"), spotify: link("spotify") },
  };
}

export function sourceLabel(source: string): string {
  return source === "soundcloud" ? t("source.soundcloud") : source === "local" ? t("source.local") : t("source.youtube");
}

export const player = new Player();
