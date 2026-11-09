<script lang="ts">
  import { coverAt, proxiedCover } from "../api";
  import { dominantColors, neutralColors, toneForBackdrop, type Rgb } from "./colors";

  // `full`: the colors fill the whole view (fullscreen player, idle screen) instead of
  // melting into near-black below the top.
  let { cover, full = false }: { cover: string | null; full?: boolean } = $props();

  // One set of blobs drifts forever; only their colors change. The colors are registered CSS
  // properties (see tokens.css), so the browser blends old → new smoothly with no flash and
  // without restarting the drift.
  let colors = $state<Rgb[]>(neutralColors());
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
    // Ignore results for covers the user already moved past.
    const apply = (next: Rgb[]) => id === seq && (colors = next);
    if (!url) apply(neutralColors());
    else extract(url).then(apply, () => apply(neutralColors()));
  });

  const css = (c: Rgb | undefined, a: number) => (c ? `rgba(${c[0]},${c[1]},${c[2]},${a})` : "transparent");
</script>

<div
  class="backdrop"
  class:full
  aria-hidden="true"
  style:--bd-1={css(colors[0], 0.9)}
  style:--bd-2={css(colors[1] ?? colors[0], 0.75)}
  style:--bd-3={css(colors[2] ?? colors[0], 0.6)}>
  <div class="blob a"></div>
  <div class="blob b"></div>
  <div class="blob c"></div>
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
    transition:
      --bd-1 600ms var(--ease),
      --bd-2 600ms var(--ease),
      --bd-3 600ms var(--ease);
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
    background: radial-gradient(closest-side, var(--bd-1), transparent);
    animation: drift-a 46s ease-in-out infinite alternate;
  }
  .b {
    left: 30vw;
    top: -50vh;
    background: radial-gradient(closest-side, var(--bd-2), transparent);
    animation: drift-b 58s ease-in-out infinite alternate;
  }
  .c {
    left: 62vw;
    top: -38vh;
    background: radial-gradient(closest-side, var(--bd-3), transparent);
    animation: drift-c 52s ease-in-out infinite alternate;
  }
  /* The colors live at the top and melt into near-black, so lists stay calm and readable. */
  .shade {
    position: absolute;
    inset: 0;
    background: linear-gradient(to bottom, rgba(10, 10, 11, 0) 0%, rgba(10, 10, 11, 0.55) 34%, var(--bg) 62%);
  }
  .full .blob {
    height: 150vh;
  }
  .full .c {
    top: 10vh;
  }
  .full .shade {
    background: linear-gradient(to bottom, rgba(10, 10, 11, 0.18), rgba(10, 10, 11, 0.5) 70%, rgba(10, 10, 11, 0.72));
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
