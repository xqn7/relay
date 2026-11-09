<script lang="ts">
  import { coverAt, proxiedCover } from "../api";
  import type { IconName } from "../icons";
  import Icon from "./Icon.svelte";

  let { src, size, radius = 6, round = false, fallback = "music" }: { src: string | null; size: number; radius?: number; round?: boolean; fallback?: IconName } =
    $props();

  let failed = $state(false);
  // Ask for roughly twice the displayed size for sharp edges on high-DPI screens.
  // Through the app, which keeps a copy on disk: covers load instantly and show offline too.
  const url = $derived.by(() => {
    const sized = coverAt(src, Math.min(544, Math.ceil(size * 2)));
    return sized ? proxiedCover(sized) : null;
  });
  $effect(() => {
    void src;
    failed = false;
  });
</script>

<div class="cover" style:width="{size}px" style:height="{size}px" style:border-radius={round ? "50%" : `${radius}px`}>
  {#if url && !failed}
    <img src={url} alt="" loading="lazy" decoding="async" draggable="false" onerror={() => (failed = true)} />
  {:else}
    <Icon name={fallback} size={Math.max(14, Math.round(size * 0.38))} />
  {/if}
</div>

<style>
  .cover {
    flex: none;
    display: grid;
    place-items: center;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.07);
    color: var(--text-3);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
