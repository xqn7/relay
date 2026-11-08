<script lang="ts">
  import { fade } from "svelte/transition";
  import { coverAt, proxiedCover } from "../api";
  import { dominantColors, neutralColors, toneForBackdrop, type Rgb } from "./colors";

  let { cover }: { cover: string | null } = $props();

  interface Layer {
    id: number;
    colors: Rgb[];
  }
  let layers = $state<Layer[]>([{ id: 0, colors: neutralColors() }]);
  let seq = 0;

  async function extract(url: string): Promise<Rgb[]> {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.src = proxiedCover(coverAt(url, 120) ?? url);
    await img.decode();
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 32;
    const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
    ctx.drawImage(img, 0, 0, 32, 32);
    return dominantColors(ctx.getImageData(0, 0, 32, 32).data, 3).map(toneForBackdrop);
  }

  $effect(() => {
    const url = cover;
    const id = ++seq;
    const done = (colors: Rgb[]) => {
      // Ignore results for covers the user already moved past.
      if (id === seq) layers = [...layers.slice(-1), { id, colors }];
    };
    if (!url) done(neutralColors());
    else extract(url).then(done, () => done(neutralColors()));
  });

  const rgba = (c: Rgb, a: number) => `rgba(${c[0]},${c[1]},${c[2]},${a})`;
</script>

<div class="backdrop" aria-hidden="true">
  {#each layers as layer (layer.id)}
    <div class="layer" in:fade={{ duration: 550 }} out:fade={{ duration: 550 }}>
      <div class="blob a" style:background="radial-gradient(closest-side, {rgba(layer.colors[0], 0.9)}, transparent)"></div>
      <div class="blob b" style:background="radial-gradient(closest-side, {rgba(layer.colors[1] ?? layer.colors[0], 0.75)}, transparent)"></div>
      <div class="blob c" style:background="radial-gradient(closest-side, {rgba(layer.colors[2] ?? layer.colors[0], 0.6)}, transparent)"></div>
    </div>
  {/each}
  <div class="shade"></div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 0;
    overflow: hidden;
    background: var(--bg);
    pointer-events: none;
  }
  .layer {
    position: absolute;
    inset: 0;
  }
  .blob {
    position: absolute;
    width: 75vw;
    height: 90vh;
    will-change: transform;
  }
  .a {
    left: -18vw;
    top: -42vh;
    animation: drift-a 46s ease-in-out infinite alternate;
  }
  .b {
    left: 30vw;
    top: -50vh;
    animation: drift-b 58s ease-in-out infinite alternate;
  }
  .c {
    left: 62vw;
    top: -38vh;
    animation: drift-c 52s ease-in-out infinite alternate;
  }
  /* The colors live at the top and melt into near-black, so lists stay calm and readable. */
  .shade {
    position: absolute;
    inset: 0;
    background: linear-gradient(to bottom, rgba(10, 10, 11, 0) 0%, rgba(10, 10, 11, 0.55) 34%, var(--bg) 62%);
  }
  @keyframes drift-a {
    to {
      transform: translate(14vw, 8vh) scale(1.12);
    }
  }
  @keyframes drift-b {
    to {
      transform: translate(-12vw, 10vh) scale(0.92);
    }
  }
  @keyframes drift-c {
    to {
      transform: translate(-10vw, 4vh) scale(1.1);
    }
  }
</style>
