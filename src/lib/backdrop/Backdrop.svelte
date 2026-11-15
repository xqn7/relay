<script lang="ts">
  import { backgrounds } from "../stores/backgrounds.svelte";
  import { player } from "../stores/player.svelte";
  import { settings, type BackgroundPlace } from "../stores/settings.svelte";
  import { visual } from "../stores/visual.svelte";
  import BackgroundImage from "./BackgroundImage.svelte";
  import { neutralColors, type Rgb } from "./colors";
  import { coverColors, rgba } from "./extract";

  // `place`: where this backdrop is drawn; a custom background image can be limited to some
  // places. The fullscreen player and the idle screen fill the whole view with color instead
  // of melting into near-black below the top.
  // `colors` can be bound by views that draw in the cover's colors too (the visualizer).
  let {
    cover,
    place = "app",
    colors = $bindable(neutralColors()),
  }: { cover: string | null; place?: BackgroundPlace; colors?: Rgb[] } = $props();

  const full = $derived(place !== "app");
  const mode = $derived(settings.values.background);
  const image = $derived(backgrounds.activeIn(place));

  // One set of blobs drifts forever; only their colors change. The colors are registered CSS
  // properties (see tokens.css), so the browser blends old → new smoothly with no flash and
  // without restarting the drift.
  let seq = 0;

  $effect(() => {
    // "Off" means no cover colors at all, so nothing is read from covers either.
    const url = mode === "off" ? null : cover;
    const id = ++seq;
    // Ignore results for covers the user already moved past.
    const apply = (next: Rgb[]) => id === seq && (colors = next);
    if (!url) apply(neutralColors());
    else coverColors(url).then(apply, () => apply(neutralColors()));
  });
</script>

<div
  class="backdrop"
  class:full
  class:resting={!player.playing}
  aria-hidden="true"
  style:--bd-1={rgba(colors[0], 0.9)}
  style:--bd-2={rgba(colors[1] ?? colors[0], 0.75)}
  style:--bd-3={rgba(colors[2] ?? colors[0], 0.6)}>
  {#if image}
    <BackgroundImage url={image.url} look={backgrounds.look(image)} {colors} still={visual.quality === "low"} />
  {:else if mode !== "off"}
    <div class="blob a"></div>
    <div class="blob b"></div>
    <div class="blob c"></div>
    <div class="shade"></div>
  {/if}
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
    opacity: var(--bd-strength);
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
    background: linear-gradient(to bottom, rgba(var(--bg-rgb), 0) 0%, rgba(var(--bg-rgb), 0.55) 34%, var(--bg) 62%);
  }
  .full .blob {
    height: 150vh;
  }
  .full .c {
    top: 10vh;
  }
  .full .shade {
    background: linear-gradient(to bottom, rgba(var(--bg-rgb), 0.18), rgba(var(--bg-rgb), 0.5) 70%, rgba(var(--bg-rgb), 0.72));
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
  /* A moving backdrop makes every glass panel above it re-blur each frame; with the music
     paused it holds still so a paused Relay costs no CPU. */
  .resting .blob {
    animation-play-state: paused;
  }
  /* Low quality: the colors stay, the drift stops. */
  :global(html[data-quality="low"]) .blob {
    animation: none;
  }
</style>
