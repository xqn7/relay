<script lang="ts">
  import type { Track } from "../api";
  import { t } from "../i18n/index.svelte";
  import { library } from "../stores/library.svelte";
  import Icon from "./Icon.svelte";

  // The mini player has no library of its own: it passes the state in and asks the main
  // window to toggle (`liked` + `ontoggle`).
  let {
    track,
    size = 18,
    quiet = false,
    liked: given,
    ontoggle,
  }: { track: Pick<Track, "id">; size?: number; quiet?: boolean; liked?: boolean; ontoggle?: () => void } = $props();
  const liked = $derived(given ?? library.liked.has(track.id));
  // Bumped on each like so the pop/burst animation restarts; unliking just empties the heart.
  let burst = $state(0);
  // Long lists reuse rows for other songs; a burst belongs to the song that was liked.
  $effect(() => {
    void track.id;
    burst = 0;
  });

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    if (!liked) burst += 1;
    if (ontoggle) ontoggle();
    else library.toggleLike(track as Track);
  }
</script>

<button
  type="button"
  class="like"
  class:liked
  class:quiet={quiet && !liked}
  aria-pressed={liked}
  aria-label={liked ? t("like.remove") : t("like.add")}
  title={liked ? t("like.remove") : t("like.add")}
  ondblclick={(e) => e.stopPropagation()}
  onclick={toggle}>
  {#key burst}
    <span class="heart" class:pop={burst > 0 && liked}><Icon name="heart" {size} fill={liked} /></span>
    {#if burst > 0 && liked}
      <span class="dots" aria-hidden="true">{#each Array(8) as _, i}<i style:--a="{i * 45}deg"></i>{/each}</span>
    {/if}
  {/key}
</button>

<style>
  .like {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-2);
    transition: color var(--t-fast) var(--ease), opacity var(--t-fast) var(--ease);
  }
  .like:hover {
    color: var(--text);
  }
  .liked {
    color: var(--text);
  }
  .quiet {
    opacity: 0;
  }
  :global(.row:hover) .quiet,
  :global(.row:focus-within) .quiet {
    opacity: 1;
  }
  .heart {
    display: grid;
  }
  .pop {
    animation: pop 340ms var(--ease);
  }
  @keyframes pop {
    0% {
      transform: scale(0.7);
    }
    45% {
      transform: scale(1.22);
    }
    100% {
      transform: scale(1);
    }
  }
  .dots {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .dots i {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 3px;
    height: 3px;
    margin: -1.5px;
    border-radius: 50%;
    background: #fff;
    opacity: 0;
    animation: burst 360ms var(--ease) forwards;
  }
  @keyframes burst {
    0% {
      opacity: 1;
      transform: rotate(var(--a)) translateY(-6px) scale(1);
    }
    100% {
      opacity: 0;
      transform: rotate(var(--a)) translateY(-15px) scale(0.4);
    }
  }
</style>
