<!-- Bars or a soft wave along the bottom of the fullscreen player, moving with the music in
     the cover's colors. Draws only while it's on screen and music plays. -->
<script lang="ts">
  import type { Rgb } from "../../lib/backdrop/colors";
  import { player } from "../../lib/stores/player.svelte";

  let { colors, style }: { colors: Rgb[]; style: "bars" | "wave" } = $props();

  const BARS = 56;
  /** Frequencies shown: the musical range, spread evenly on a log scale. */
  const LOW_HZ = 40;
  const HIGH_HZ = 14000;

  let canvas = $state<HTMLCanvasElement>();
  let width = $state(0);
  let height = $state(0);
  const levels = new Float32Array(BARS);

  function mix(t: number): Rgb {
    const c = colors.length > 0 ? colors : [[255, 255, 255] as Rgb];
    const pos = t * (c.length - 1);
    const i = Math.min(c.length - 2, Math.floor(pos));
    if (c.length === 1) return c[0];
    const f = pos - i;
    return [0, 1, 2].map((k) => Math.round(c[i][k] + (c[i + 1][k] - c[i][k]) * f)) as Rgb;
  }

  /** Reads the analyser into `levels` (0–1), log-spaced; false when everything is silent. */
  function read(an: AnalyserNode, data: Uint8Array<ArrayBuffer>, rate: number): boolean {
    an.getByteFrequencyData(data);
    const binHz = rate / an.fftSize;
    let any = false;
    for (let i = 0; i < BARS; i++) {
      const lo = LOW_HZ * Math.pow(HIGH_HZ / LOW_HZ, i / BARS);
      const hi = LOW_HZ * Math.pow(HIGH_HZ / LOW_HZ, (i + 1) / BARS);
      let peak = 0;
      for (let b = Math.floor(lo / binHz); b <= Math.min(data.length - 1, Math.ceil(hi / binHz)); b++) peak = Math.max(peak, data[b]);
      // Falls slowly, rises at once: calmer than raw analyser values.
      const v = peak / 255;
      levels[i] = v > levels[i] ? v : levels[i] * 0.9 + v * 0.1;
      if (levels[i] > 0.01) any = true;
    }
    return any;
  }

  function draw(ctx: CanvasRenderingContext2D, w: number, h: number) {
    ctx.clearRect(0, 0, w, h);
    if (style === "bars") {
      const gap = w / BARS;
      const bw = Math.max(2, gap * 0.55);
      for (let i = 0; i < BARS; i++) {
        const bh = Math.max(2, levels[i] * levels[i] * h);
        const [r, g, b] = mix(i / (BARS - 1));
        ctx.fillStyle = `rgba(${r},${g},${b},0.3)`;
        ctx.beginPath();
        ctx.roundRect(i * gap + (gap - bw) / 2, h - bh, bw, bh, Math.min(bw / 2, 3));
        ctx.fill();
      }
      return;
    }
    // Two soft layers: the cover's main color behind, its second color in front.
    for (const [layer, scale, alpha] of [[0, 1, 0.2], [1, 0.7, 0.26]] as const) {
      const [r, g, b] = mix(layer === 0 ? 0 : 0.5);
      ctx.fillStyle = `rgba(${r},${g},${b},${alpha})`;
      ctx.beginPath();
      ctx.moveTo(0, h);
      const step = w / (BARS - 1);
      let px = 0;
      let py = h - levels[0] * levels[0] * h * scale;
      ctx.lineTo(px, py);
      for (let i = 1; i < BARS; i++) {
        const x = i * step;
        const y = h - levels[i] * levels[i] * h * scale;
        ctx.quadraticCurveTo(px, py, (px + x) / 2, (py + y) / 2);
        px = x;
        py = y;
      }
      ctx.lineTo(w, py);
      ctx.lineTo(w, h);
      ctx.closePath();
      ctx.fill();
    }
  }

  $effect(() => {
    const c = canvas;
    const playing = player.playing;
    const w = width;
    const h = height;
    void style;
    void colors;
    if (!c || w === 0 || h === 0) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(w * dpr);
    c.height = Math.round(h * dpr);
    const ctx = c.getContext("2d")!;
    ctx.scale(dpr, dpr);
    const an = player.engine.analyser();
    if (!an) return;
    const data = new Uint8Array(an.frequencyBinCount);
    const rate = an.context.sampleRate;
    let frame = 0;
    // Keeps drawing after a pause only until the bars have settled.
    const tick = () => {
      const moving = read(an, data, rate);
      draw(ctx, w, h);
      if (playing || moving) frame = requestAnimationFrame(tick);
    };
    tick();
    return () => cancelAnimationFrame(frame);
  });
</script>

<canvas class="viz" bind:this={canvas} bind:clientWidth={width} bind:clientHeight={height} aria-hidden="true"></canvas>

<style>
  .viz {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    width: 100%;
    height: 18vh;
    pointer-events: none;
  }
</style>
