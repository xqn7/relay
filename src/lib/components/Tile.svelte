<!-- A square library tile (playlist, folder, album, artist). The art itself reacts when
     something dragged can be dropped onto it. -->
<script lang="ts">
  import type { IconName } from "../icons";
  import { dnd, dropTarget, type DropHandler } from "../stores/dnd.svelte";
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";

  let {
    title,
    subtitle = "",
    cover = null,
    covers = [],
    icon = "music",
    round = false,
    drop,
    onclick,
    onmenu,
    onpress,
  }: {
    title: string;
    subtitle?: string;
    cover?: string | null;
    /** Up to four covers drawn as a grid (folders). */
    covers?: (string | null)[];
    /** Shown instead of a cover (Liked Songs, Local music, empty playlists). */
    icon?: IconName;
    round?: boolean;
    drop?: { key: string } & DropHandler;
    onclick: () => void;
    onmenu?: (e: MouseEvent) => void;
    onpress?: (e: PointerEvent) => void;
  } = $props();

  const over = $derived(!!drop && dnd.target === drop.key);
  const noDrop = { key: "", accepts: () => false, drop: () => {} };
</script>

<div
  class="tile"
  role="button"
  tabindex="0"
  onclick={onclick}
  onkeydown={(e) => e.key === "Enter" && onclick()}
  oncontextmenu={(e) => onmenu?.(e)}
  onpointerdown={(e) => onpress?.(e)}>
  <div class="art" class:round class:over use:dropTarget={drop ?? { ...noDrop, key: `none:${title}` }}>
    {#if covers.filter(Boolean).length >= 2}
      <div class="grid">
        {#each covers.slice(0, 4) as c, i (i)}<Cover src={c} size={88} radius={0} />{/each}
      </div>
    {:else if cover || covers[0]}
      <Cover src={cover ?? covers[0]} size={180} radius={0} />
    {:else}
      <div class="icon"><Icon name={icon} size={46} strokeWidth={1.6} /></div>
    {/if}
  </div>
  <div class="title truncate">{title}</div>
  {#if subtitle}<div class="sub truncate">{subtitle}</div>{/if}
</div>

<style>
  .tile {
    min-width: 0;
    padding: 8px;
    border-radius: var(--r-lg);
    outline: none;
    transition: background var(--t-fast) var(--ease);
  }
  .tile:hover,
  .tile:focus-visible {
    background: var(--hover);
  }
  .art {
    position: relative;
    aspect-ratio: 1;
    overflow: hidden;
    border-radius: var(--r-md);
    background: rgba(255, 255, 255, 0.06);
    transition: transform var(--t) var(--ease), box-shadow var(--t) var(--ease);
  }
  .art :global(.cover) {
    width: 100% !important;
    height: 100% !important;
  }
  .art.round {
    border-radius: 50%;
  }
  .art.over {
    transform: scale(1.04);
    box-shadow: 0 0 0 2px rgba(255, 255, 255, 0.85);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    width: 100%;
    height: 100%;
  }
  .icon {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    color: rgba(255, 255, 255, 0.78);
    background: rgba(255, 255, 255, 0.07);
  }
  .title {
    margin-top: 10px;
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .sub {
    margin-top: 2px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
</style>
