// Audio engine: two decks (one playing, one preloading the next song) feeding one shared chain.
//   <audio> → MediaElementSource → normalize gain → fade gain ┐
//   <audio> → MediaElementSource → normalize gain → fade gain ┴→ bus → [pitch] → 10-band EQ
//     → preamp → master volume → output device        (the preamp also feeds the visualizer)
// Speed is each <audio> element's own pitch-keeping time-stretch; pitch is a small worklet
// that is only in the chain while pitch isn't 0.
// The engine knows nothing about queues; the player tells it what to load and preload.
import { BANDS, preampDb, semitoneRatio } from "./eq";
import workletUrl from "./pitch-worklet.js?url";

const FADE_S = 0.2;
const SEEK_FADE_S = 0.12;
const GAPLESS_LEAD_S = 0.3;
const STALL_LIMIT_MS = 20_000;
const LOUDNESS_TARGET_DB = -15;
const MAX_BOOST_DB = 3;
const MAX_CUT_DB = -10;
/** How far ahead a jump (loop end, skipped section) gets its exact timer. */
const JUMP_LEAD_S = 0.6;

/** Playing past `fromS` continues at `toS`: the end of an A–B loop, or a skipped section. */
export interface Jump {
  fromS: number;
  toS: number;
}

export interface EngineEvents {
  time(positionMs: number, durationMs: number): void;
  /** The next deck took over by itself (crossfade or gapless). */
  advanced(): void;
  /** The song ended (or reached its trimmed end) and no next song was preloaded. */
  ended(): void;
  state(playing: boolean, buffering: boolean): void;
  error(reason: "load" | "stall"): void;
  /** A good moment to make sure the next song is preloaded. */
  nearEnd(): void;
}

export interface LoadOptions {
  startMs?: number;
  /** A trimmed song's end. */
  endMs?: number | null;
  autoplay: boolean;
  loudnessDb: number | null;
}

interface Deck {
  el: HTMLAudioElement;
  norm: GainNode;
  fade: GainNode;
  analyser: AnalyserNode;
  url: string | null;
  /** Pause events we caused ourselves and should not treat as an outside pause. */
  ownPauses: number;
  /** Loudness samples (dBFS) taken while the song plays, when the service gave none. */
  samples: number[] | null;
  /** A trimmed song ends here (seconds) instead of at its real end. */
  endS: number | null;
}

type SinkContext = AudioContext & { setSinkId(id: string): Promise<void> };

const dbToGain = (db: number) => Math.pow(10, db / 20);
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export class AudioEngine {
  private ctx: AudioContext | null = null;
  private bus: GainNode | null = null;
  private eq: BiquadFilterNode[] = [];
  private preamp: GainNode | null = null;
  private master: GainNode | null = null;
  private pitchNode: AudioWorkletNode | null = null;
  private workletLoaded: Promise<void> | null = null;
  private tap: AnalyserNode | null = null;
  private decks: Deck[] = [];
  private active = 0;
  private nextReady = false;
  private transitioning = false;
  private nearEndSent = false;
  private gaplessTimer = 0;
  private stallTimer = 0;
  private jumpTimer = 0;
  private jumps: Jump[] = [];
  private volume = 0.8;
  private muted = false;
  private wantPlaying = false;
  private speed = 1;
  private semitones = 0;
  private eqOn = false;
  private eqGains: number[] = BANDS.map(() => 0);
  private sinkId = "";

  crossfadeS = 3;
  normalize = true;

  constructor(private events: EngineEvents) {}

  /** Built lazily: browsers only allow audio to start after the user interacts. */
  private ensure(): void {
    if (this.ctx) return;
    const ctx = new AudioContext({ latencyHint: "playback" });
    this.ctx = ctx;
    this.bus = ctx.createGain();
    this.eq = BANDS.map((hz, i) => {
      const f = ctx.createBiquadFilter();
      f.type = i === 0 ? "lowshelf" : i === BANDS.length - 1 ? "highshelf" : "peaking";
      f.frequency.value = hz;
      // Bands sit an octave apart; this Q makes each one about an octave wide.
      f.Q.value = 1.4;
      return f;
    });
    this.preamp = ctx.createGain();
    this.master = ctx.createGain();
    this.eq.reduce<AudioNode>((prev, f) => prev.connect(f), this.bus);
    this.eq[this.eq.length - 1].connect(this.preamp).connect(this.master).connect(ctx.destination);
    this.applyVolume();
    this.applyEq();
    this.decks = [this.makeDeck(0), this.makeDeck(1)];
    if (this.sinkId) this.applySink().catch(() => {});
    if (this.semitones !== 0) this.applyPitch().catch(() => {});
  }

  private makeDeck(i: number): Deck {
    const ctx = this.ctx!;
    const el = new Audio();
    el.crossOrigin = "anonymous";
    el.preload = "auto";
    el.preservesPitch = true;
    el.defaultPlaybackRate = el.playbackRate = this.speed;
    const source = ctx.createMediaElementSource(el);
    const norm = ctx.createGain();
    const fade = ctx.createGain();
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 2048;
    source.connect(norm).connect(fade).connect(this.bus!);
    source.connect(analyser);
    const deck: Deck = { el, norm, fade, analyser, url: null, ownPauses: 0, samples: null, endS: null };
    el.addEventListener("timeupdate", () => i === this.active && this.onTime(deck));
    el.addEventListener("ended", () => i === this.active && this.onEnded());
    el.addEventListener("waiting", () => i === this.active && this.setBuffering(true));
    el.addEventListener("playing", () => i === this.active && this.setBuffering(false));
    // Something outside the app paused the audio (or the system did); keep the UI truthful.
    el.addEventListener("pause", () => {
      if (deck.ownPauses > 0) {
        deck.ownPauses -= 1;
        return;
      }
      if (i !== this.active || !this.wantPlaying || this.transitioning || el.ended || !deck.url) return;
      this.wantPlaying = false;
      this.events.state(false, false);
    });
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

  get positionMs(): number {
    return this.ctx ? this.cur.el.currentTime * 1000 : 0;
  }

  /** Loads a song into the active deck, fading out whatever played before. */
  async load(url: string, opts: LoadOptions): Promise<void> {
    this.ensure();
    this.cancelTimers();
    const old = this.cur;
    if (!old.el.paused) await this.fadeTo(old, 0, FADE_S);
    this.quietPause(old);
    this.clearNext();
    this.prepareDeck(old, url, opts.loudnessDb, opts.endMs ?? null);
    if (opts.startMs) await this.seekWhenReady(old, opts.startMs / 1000);
    this.wantPlaying = opts.autoplay;
    if (opts.autoplay) await this.start(old, FADE_S);
    else this.events.state(false, false);
  }

  /** Loads the following song into the idle deck so it can start without a gap. */
  preloadNext(url: string, loudnessDb: number | null, startMs = 0, endMs: number | null = null): void {
    if (!this.ctx || this.transitioning) return;
    const deck = this.other;
    if (deck.url === url && this.nextReady) return;
    this.prepareDeck(deck, url, loudnessDb, endMs);
    if (startMs > 0) this.seekWhenReady(deck, startMs / 1000);
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

  private prepareDeck(deck: Deck, url: string, loudnessDb: number | null, endMs: number | null): void {
    deck.url = url;
    deck.el.src = url;
    deck.el.load();
    deck.endS = endMs == null ? null : endMs / 1000;
    deck.fade.gain.cancelScheduledValues(0);
    deck.fade.gain.value = 0;
    deck.samples = loudnessDb == null ? [] : null;
    const gainDb = loudnessDb == null ? 0 : clamp(-loudnessDb, MAX_CUT_DB, MAX_BOOST_DB);
    deck.norm.gain.value = this.normalize ? dbToGain(gainDb) : 1;
    this.nearEndSent = false;
  }

  /** Stops and empties the active deck, e.g. after a song failed for good. */
  unload(): void {
    if (!this.ctx) return;
    this.cancelTimers();
    this.clearNext();
    const deck = this.cur;
    deck.url = null;
    this.quietPause(deck);
    deck.el.removeAttribute("src");
    deck.el.load();
    this.wantPlaying = false;
    this.events.state(false, false);
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
    this.cancelTimers();
    this.events.state(false, false);
    await this.fadeTo(this.cur, 0, FADE_S);
    if (!this.wantPlaying) this.quietPause(this.cur);
  }

  async seek(ms: number): Promise<void> {
    if (!this.ctx || !this.cur.url) return;
    this.cancelTimers();
    this.clearNext();
    const deck = this.cur;
    const wasPlaying = !deck.el.paused;
    if (wasPlaying) await this.fadeTo(deck, 0, SEEK_FADE_S);
    deck.el.currentTime = Math.max(0, ms / 1000);
    this.nearEndSent = false;
    if (wasPlaying) this.fadeTo(deck, 1, SEEK_FADE_S);
  }

  /** Changes the playing song's trimmed end (null = its real end). */
  setEnd(endMs: number | null): void {
    if (!this.ctx) return;
    this.cur.endS = endMs == null ? null : endMs / 1000;
    this.cancelTimers();
  }

  /** Places in the current song where playback jumps ahead or back. */
  setJumps(jumps: Jump[]): void {
    this.jumps = jumps;
    clearTimeout(this.jumpTimer);
    this.jumpTimer = 0;
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

  setEq(on: boolean, gains: number[]): void {
    this.eqOn = on;
    this.eqGains = gains;
    this.applyEq();
  }

  private applyEq(): void {
    if (!this.ctx) return;
    const now = this.ctx.currentTime;
    // Short glides, so dragging a band doesn't click.
    this.eq.forEach((f, i) => f.gain.setTargetAtTime(this.eqOn ? this.eqGains[i] : 0, now, 0.03));
    this.preamp!.gain.setTargetAtTime(dbToGain(this.eqOn ? preampDb(this.eqGains) : 0), now, 0.03);
  }

  /** 0.5–2×, pitch kept. */
  setSpeed(rate: number): void {
    this.speed = rate;
    for (const d of this.decks) {
      d.el.defaultPlaybackRate = rate;
      d.el.playbackRate = rate;
    }
    // Jump and end timers were set for the old speed.
    this.cancelTimers();
  }

  /** Semitones up or down, speed unchanged. */
  async setPitch(semitones: number): Promise<void> {
    this.semitones = semitones;
    if (this.ctx) await this.applyPitch();
  }

  private async applyPitch(): Promise<void> {
    const ctx = this.ctx!;
    if (this.semitones !== 0 && !this.pitchNode) {
      this.workletLoaded ??= ctx.audioWorklet.addModule(workletUrl);
      await this.workletLoaded;
      if (this.pitchNode) return;
      this.pitchNode = new AudioWorkletNode(ctx, "pitch-shifter", { outputChannelCount: [2] });
    }
    const ratio = semitoneRatio(this.semitones);
    this.pitchNode?.parameters.get("ratio")!.setValueAtTime(ratio, ctx.currentTime);
    // Only in the chain while it does something, so pitch 0 costs nothing.
    const bus = this.bus!;
    bus.disconnect();
    this.pitchNode?.disconnect();
    if (this.semitones !== 0 && this.pitchNode) bus.connect(this.pitchNode).connect(this.eq[0]);
    else bus.connect(this.eq[0]);
  }

  /** Plays through a specific output device ("" = the Windows default). */
  async setSink(id: string): Promise<void> {
    this.sinkId = id;
    if (this.ctx) await this.applySink();
  }

  private async applySink(): Promise<void> {
    await (this.ctx as SinkContext).setSinkId(this.sinkId);
  }

  /** What's playing, after the EQ, for the visualizer. */
  analyser(): AnalyserNode | null {
    if (!this.ctx) return null;
    if (!this.tap) {
      this.tap = this.ctx.createAnalyser();
      this.tap.fftSize = 512;
      this.tap.smoothingTimeConstant = 0.75;
      // A wider window than the default, so loud masters don't pin every bar to the top.
      this.tap.minDecibels = -90;
      this.tap.maxDecibels = -12;
      this.preamp!.connect(this.tap);
    }
    return this.tap;
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

  private quietPause(deck: Deck): void {
    if (deck.el.paused) return;
    deck.ownPauses += 1;
    deck.el.pause();
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
    const end = deck.endS != null && deck.endS < dur ? deck.endS : dur;
    // Timers run in real time, so media seconds are divided by the playback speed.
    const remaining = (end - pos) / this.speed;
    if (!this.nearEndSent && remaining < Math.max(30, this.crossfadeS + 20)) {
      this.nearEndSent = true;
      this.events.nearEnd();
    }
    if (!this.wantPlaying) return;
    this.armJump(pos);
    const trimmed = end < dur;
    const fade = Math.min(this.crossfadeS, end / 3);
    if (this.nextReady && fade > 0 && remaining <= fade) this.transition(fade);
    else if ((this.nextReady || trimmed) && remaining <= GAPLESS_LEAD_S && !this.gaplessTimer) {
      this.gaplessTimer = window.setTimeout(() => this.reachEnd(), Math.max(0, remaining * 1000 - 20));
    }
  }

  /** The current song reached its (trimmed) end: hand over, or report the end. */
  private reachEnd(): void {
    this.gaplessTimer = 0;
    if (this.nextReady && this.wantPlaying) {
      this.transition(0);
      return;
    }
    this.wantPlaying = false;
    this.quietPause(this.cur);
    this.events.ended();
  }

  /** Sets an exact timer for a jump coming up within the next moment. */
  private armJump(pos: number): void {
    if (this.jumpTimer) return;
    for (const j of this.jumps) {
      const ahead = (j.fromS - pos) / this.speed;
      if (ahead > -0.25 && ahead <= JUMP_LEAD_S) {
        // Starts the short fade-out early, so the jump happens right at `fromS`.
        this.jumpTimer = window.setTimeout(() => this.jumpTo(j.toS), Math.max(0, (ahead - SEEK_FADE_S) * 1000));
        return;
      }
    }
  }

  private async jumpTo(toS: number): Promise<void> {
    const deck = this.cur;
    if (!deck.url || deck.el.paused) {
      this.jumpTimer = 0;
      return;
    }
    await this.fadeTo(deck, 0, SEEK_FADE_S);
    deck.el.currentTime = toS;
    this.jumpTimer = 0;
    this.fadeTo(deck, 1, SEEK_FADE_S);
  }

  /** Hands playback to the preloaded deck: a crossfade, or a back-to-back start when fade is 0. */
  private transition(fadeS: number): void {
    if (this.transitioning) return;
    this.transitioning = true;
    this.cancelTimers();
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

  private cancelTimers(): void {
    clearTimeout(this.gaplessTimer);
    clearTimeout(this.jumpTimer);
    this.gaplessTimer = 0;
    this.jumpTimer = 0;
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
