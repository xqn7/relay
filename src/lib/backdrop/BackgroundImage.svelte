<!-- A custom background image with its look (blur, dim, brightness, saturation, zoom, position)
     and, if wanted, tinted with the cover colors. Used by the backdrop and the settings preview. -->
<script lang="ts">
  import type { Rgb } from "./colors";
  import { rgba } from "./extract";
  import { lookFilter, type Look } from "./look";

  // `still`: animated GIFs hold their first frame (Low visual quality and Game mode).
  let { url, look, colors = [], still = false }: { url: string; look: Look; colors?: Rgb[]; still?: boolean } = $props();

  let frame = $state<string | null>(null);

  $effect(() => {
    frame = null;
    if (!still || !url.toLowerCase().endsWith(".gif")) return;
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      const canvas = document.createElement("canvas");
      canvas.width = img.naturalWidth;
      canvas.height = img.naturalHeight;
      canvas.getContext("2d")?.drawImage(img, 0, 0);
      canvas.toBlob((b) => b && (frame = URL.createObjectURL(b)));
    };
    img.src = url;
    return () => {
      img.onload = null;
      if (frame) URL.revokeObjectURL(frame);
    };
  });
</script>

<div class="bg" aria-hidden="true">
  <!-- Pulled out past the edges by the blur radius, so blurred edges don't fade to black. -->
  <img
    src={still && frame ? frame : url}
    alt=""
    draggable="false"
    style:left="-{look.blur * 2}px"
    style:top="-{look.blur * 2}px"
    style:width="calc(100% + {look.blur * 4}px)"
    style:height="calc(100% + {look.blur * 4}px)"
    style:filter={lookFilter(look)}
    style:object-position="{look.x}% {look.y}%"
    style:transform="scale({look.zoom})"
    style:transform-origin="{look.x}% {look.y}%" />
  {#if look.mix && colors.length}
    <div
      class="tint"
      style:--bd-1={rgba(colors[0], 1)}
      style:--bd-2={rgba(colors[1] ?? colors[0], 1)}
      style:--bd-3={rgba(colors[2] ?? colors[0], 1)}>
    </div>
  {/if}
  <div class="dim" style:opacity={look.dim}></div>
</div>

<style>
  .bg {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }
  img {
    position: absolute;
    object-fit: cover;
  }
  /* Takes the image's light and dark, and the cover's hue. */
  .tint {
    position: absolute;
    inset: 0;
    opacity: 0.7;
    mix-blend-mode: color;
    background:
      radial-gradient(60% 70% at 15% 10%, var(--bd-1), transparent),
      radial-gradient(60% 70% at 85% 20%, var(--bd-2), transparent),
      radial-gradient(70% 70% at 50% 100%, var(--bd-3), transparent);
    transition:
      --bd-1 600ms var(--ease),
      --bd-2 600ms var(--ease),
      --bd-3 600ms var(--ease);
  }
  .dim {
    position: absolute;
    inset: 0;
    background: var(--bg);
  }
</style>
