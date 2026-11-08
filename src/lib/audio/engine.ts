// Audio engine: two decks (one playing, one preloading the next song) feeding one shared chain.
//   <audio> → MediaElementSource → normalize gain → fade gain ┐
//   <audio> → MediaElementSource → normalize gain → fade gain ┴→ master volume → speakers
// The engine knows nothing about queues; the player tells it what to load and preload.

const FADE_S = 0.2;
const SEEK_FADE_S = 0.12;
const GAPLESS_LEAD_S = 0.3;
const STALL_LIMIT_MS = 20_000;
const LOUDNESS_TARGET_DB = -15;
const MAX_BOOST_DB = 3;
const MAX_CUT_DB = -10;

export interface EngineEvents {
  time(positionMs: number, durationMs: number): void;
  /** The next deck took over by itself (crossfade or gapless). */
  advanced(): void;
  /** The song ended and no next song was preloaded. */
  ended(): void;
  state(playing: boolean, buffering: boolean): void;
  error(reason: "load" | "stall"): void;
  /** A good moment to make sure the next song is preloaded. */
  nearEnd(): void;
}

interface Deck {
  el: HTMLAudioElement;
  norm: GainNode;
  fade: GainNode;
  analyser: AnalyserNode;
  url: string | null;
  /** Loudness samples (dBFS) taken while the song plays, when the service gave none. */
  samples: number[] | null;
}

const dbToGain = (db: number) => Math.pow(10, db / 20);
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export class AudioEngine {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private decks: Deck[] = [];
  private active = 0;
  private nextReady = false;
  private transitioning = false;
  private nearEndSent = false;
  private gaplessTimer = 0;
  private stallTimer = 0;
  private volume = 0.8;
  private muted = false;
  private wantPlaying = false;

  crossfadeS = 3;
  normalize = true;

  constructor(private events: EngineEvents) {}

  /** Built lazily: browsers only allow audio to start after the user interacts. */
  private ensure(): void {
    if (this.ctx) return;
    this.ctx = new AudioContext({ latencyHint: "playback" });
    this.master = this.ctx.createGain();
    this.master.connect(this.ctx.destination);
    this.applyVolume();
    this.decks = [this.makeDeck(0), this.makeDeck(1)];
  }

  private makeDeck(i: number): Deck {
    const ctx = this.ctx!;
    const el = new Audio();
    el.crossOrigin = "anonymous";
    el.preload = "auto";
    const source = ctx.createMediaElementSource(el);
    const norm = ctx.createGain();
    const fade = ctx.createGain();
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 2048;
    source.connect(norm).connect(fade).connect(this.master!);
    source.connect(analyser);
    const deck: Deck = { el, norm, fade, analyser, url: null, samples: null };
    el.addEventListener("timeupdate", () => i === this.active && this.onTime(deck));
    el.addEventListener("ended", () => i === this.active && this.onEnded());
    el.addEventListener("waiting", () => i === this.active && this.setBuffering(true));
    el.addEventListener("playing", () => i === this.active && this.setBuffering(false));
    el.addEventListener("error", () => {
      if (i === this.active && deck.url) this.events.error("load");
      else if (i !== this.active) this.nextReady = false;
    });
    return deck;
  }

  private get cur(): Deck {
    return this.decks[this.active];
  }
  private get other(): Deck {
    return this.decks[1 - this.active];
  }

  get playing(): boolean {
    return this.wantPlaying;
  }

  get positionMs(): number {
    return this.ctx ? this.cur.el.currentTime * 1000 : 0;
  }

  get durationMs(): number {
    const d = this.ctx ? this.cur.el.duration : NaN;
    return Number.isFinite(d) ? d * 1000 : 0;
  }

  /** Loads a song into the active deck, fading out whatever played before. */
  async load(url: string, opts: { startMs?: number; autoplay: boolean; loudnessDb: number | null }): Promise<void> {
    this.ensure();
    this.cancelTransition();
    const old = this.cur;
    if (!old.el.paused) await this.fadeTo(old, 0, FADE_S);
    old.el.pause();
    this.clearNext();
    this.prepareDeck(old, url, opts.loudnessDb);
    if (opts.startMs) await this.seekWhenReady(old, opts.startMs / 1000);
    this.wantPlaying = opts.autoplay;
    if (opts.autoplay) await this.start(old, FADE_S);
    else this.events.state(false, false);
  }

  /** Loads the following song into the idle deck so it can start without a gap. */
  preloadNext(url: string, loudnessDb: number | null): void {
    if (!this.ctx || this.transitioning) return;
    const deck = this.other;
    if (deck.url === url && this.nextReady) return;
    this.prepareDeck(deck, url, loudnessDb);
    this.nextReady = true;
  }

  clearNext(): void {
    if (!this.ctx) return;
    const deck = this.other;
    deck.el.pause();
    deck.el.removeAttribute("src");
    deck.el.load();
    deck.url = null;
    this.nextReady = false;
  }

  private prepareDeck(deck: Deck, url: string, loudnessDb: number | null): void {
    deck.url = url;
    deck.el.src = url;
    deck.el.load();
    deck.fade.gain.cancelScheduledValues(0);
    deck.fade.gain.value = 0;
    deck.samples = loudnessDb == null ? [] : null;
    const gainDb = loudnessDb == null ? 0 : clamp(-loudnessDb, MAX_CUT_DB, MAX_BOOST_DB);
    deck.norm.gain.value = this.normalize ? dbToGain(gainDb) : 1;
    this.nearEndSent = false;
  }

  async play(): Promise<void> {
    if (!this.ctx || !this.cur.url) return;
    this.wantPlaying = true;
    await this.ctx.resume();
    await this.start(this.cur, FADE_S);
  }

  async pause(): Promise<void> {
    if (!this.ctx) return;
    this.wantPlaying = false;
    this.cancelTransition();
    this.events.state(false, false);
    await this.fadeTo(this.cur, 0, FADE_S);
    if (!this.wantPlaying) this.cur.el.pause();
  }

  async seek(ms: number): Promise<void> {
    if (!this.ctx || !this.cur.url) return;
    this.cancelTransition();
    this.clearNext();
    const deck = this.cur;
    const wasPlaying = !deck.el.paused;
    if (wasPlaying) await this.fadeTo(deck, 0, SEEK_FADE_S);
    deck.el.currentTime = Math.max(0, ms / 1000);
    this.nearEndSent = false;
    if (wasPlaying) this.fadeTo(deck, 1, SEEK_FADE_S);
  }

  setVolume(v: number): void {
    this.volume = clamp(v, 0, 1);
    this.applyVolume();
  }

  setMuted(m: boolean): void {
    this.muted = m;
    this.applyVolume();
  }

  /** Squared so the slider feels even: loudness is perceived logarithmically. */
  private applyVolume(): void {
    if (!this.master || !this.ctx) return;
    const target = this.muted ? 0 : this.volume * this.volume;
    this.master.gain.setTargetAtTime(target, this.ctx.currentTime, 0.03);
  }

  setNormalize(on: boolean): void {
    this.normalize = on;
    if (!on) for (const d of this.decks) d.norm.gain.value = 1;
  }

  private async start(deck: Deck, fadeS: number): Promise<void> {
    await this.ctx!.resume();
    try {
      await deck.el.play();
    } catch (e) {
      // A newer load() or pause() interrupted this play(); that's expected, not an error.
      if (e instanceof DOMException && e.name === "AbortError") return;
      this.events.error("load");
      return;
    }
    this.events.state(true, false);
    this.fadeTo(deck, 1, fadeS);
  }

  private fadeTo(deck: Deck, value: number, seconds: number): Promise<void> {
    const g = deck.fade.gain;
    const now = this.ctx!.currentTime;
    g.cancelScheduledValues(now);
    g.setValueAtTime(g.value, now);
    g.linearRampToValueAtTime(value, now + seconds);
    return new Promise((r) => setTimeout(r, seconds * 1000));
  }

  private seekWhenReady(deck: Deck, seconds: number): Promise<void> {
    return new Promise((resolve) => {
      const apply = () => {
        deck.el.currentTime = seconds;
        resolve();
      };
      if (deck.el.readyState >= 1) apply();
      else deck.el.addEventListener("loadedmetadata", apply, { once: true });
    });
  }

  private setBuffering(on: boolean): void {
    clearTimeout(this.stallTimer);
    if (on && this.wantPlaying) {
      this.stallTimer = window.setTimeout(() => this.events.error("stall"), STALL_LIMIT_MS);
    }
    this.events.state(this.wantPlaying, on && this.wantPlaying);
  }

  private onTime(deck: Deck): void {
    const pos = deck.el.currentTime;
    const dur = deck.el.duration;
    this.events.time(pos * 1000, Number.isFinite(dur) ? dur * 1000 : 0);
    if (deck.samples && deck.samples.length < 24 && !deck.el.paused) this.sampleLoudness(deck);
    if (!Number.isFinite(dur) || dur <= 0 || this.transitioning) return;
    const remaining = dur - pos;
    if (!this.nearEndSent && remaining < Math.max(30, this.crossfadeS + 20)) {
      this.nearEndSent = true;
      this.events.nearEnd();
    }
    if (!this.nextReady || !this.wantPlaying) return;
    const fade = Math.min(this.crossfadeS, dur / 3);
    if (fade > 0 && remaining <= fade) this.transition(fade);
    else if (fade === 0 && remaining <= GAPLESS_LEAD_S && !this.gaplessTimer) {
      this.gaplessTimer = window.setTimeout(() => this.transition(0), Math.max(0, remaining * 1000 - 20));
    }
  }

  /** Hands playback to the preloaded deck: a crossfade, or a back-to-back start when fade is 0. */
  private transition(fadeS: number): void {
    if (this.transitioning) return;
    this.transitioning = true;
    this.gaplessTimer = 0;
    const from = this.cur;
    const to = this.other;
    this.active = 1 - this.active;
    this.nextReady = false;
    this.nearEndSent = false;
    this.events.advanced();
    const ramp = Math.max(fadeS, 0.02);
    this.start(to, ramp);
    this.fadeTo(from, 0, ramp).then(() => {
      from.el.pause();
      this.transitioning = false;
    });
  }

  private cancelTransition(): void {
    clearTimeout(this.gaplessTimer);
    this.gaplessTimer = 0;
  }

  private onEnded(): void {
    if (this.transitioning) return;
    if (this.nextReady && this.wantPlaying) {
      this.transition(0);
      return;
    }
    this.events.ended();
  }

  /** Estimates loudness from what's playing, for sources that don't report it. */
  private sampleLoudness(deck: Deck): void {
    const buf = new Float32Array(deck.analyser.fftSize);
    deck.analyser.getFloatTimeDomainData(buf);
    let sum = 0;
    for (const v of buf) sum += v * v;
    const rms = Math.sqrt(sum / buf.length);
    if (rms < 0.003) return; // silence says nothing about loudness
    deck.samples!.push(20 * Math.log10(rms));
    if (deck.samples!.length % 6 === 0 && this.normalize) {
      const avg = deck.samples!.reduce((a, b) => a + b, 0) / deck.samples!.length;
      const gainDb = clamp(LOUDNESS_TARGET_DB - avg, MAX_CUT_DB, MAX_BOOST_DB);
      deck.norm.gain.setTargetAtTime(dbToGain(gainDb), this.ctx!.currentTime, 0.8);
    }
  }
}
