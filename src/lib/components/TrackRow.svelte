<script lang="ts">
  import { artistLine, formatTime, type Track } from "../api";
  import { t } from "../i18n/index.svelte";
  import { player } from "../stores/player.svelte";
  import { openTrackMenu } from "../trackMenu";
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";
  import SourceBadge from "./SourceBadge.svelte";

  let { tracks, index, compact = false }: { tracks: Track[]; index: number; compact?: boolean } = $props();
  const track = $derived(tracks[index]);
  const isCurrent = $derived(player.current?.track.id === track.id);

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") player.activate(tracks, index);
  }
</script>

<div
  class="row"
  class:current={isCurrent}
  class:compact
  role="button"
  tabindex="0"
  ondblclick={() => player.activate(tracks, index)}
  onkeydown={onKey}
  oncontextmenu={(e) => openTrackMenu(e, tracks, index)}>
  <div class="art">
    <Cover src={track.cover} size={compact ? 40 : 44} />
    {#if isCurrent && player.playing}
      <span class="eq" aria-hidden="true"><i></i><i></i><i></i></span>
    {/if}
  </div>
  <div class="text">
    <div class="title truncate">{track.title}</div>
    <div class="artist">
      <SourceBadge source={track.source} />
      <span class="truncate">{artistLine(track)}</span>
    </div>
  </div>
  <span class="time">{formatTime(track.durationMs)}</span>
  <button class="more" aria-label={t("menu.more")} onclick={(e) => openTrackMenu(e, tracks, index)}>
    <Icon name="more" size={18} />
  </button>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: var(--s3);
    padding: 6px var(--s2) 6px 8px;
    border-radius: var(--r-md);
    outline: none;
    transition: background var(--t-fast) var(--ease);
  }
  .row:hover,
  .row:focus-visible {
    background: var(--hover);
  }
  .compact {
    padding: 4px 6px;
    gap: 10px;
  }
  .art {
    position: relative;
  }
  .text {
    min-width: 0;
  }
  .title {
    font-size: var(--fs-md);
    font-weight: 500;
  }
  .current .title {
    font-weight: 650;
  }
  .artist {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    margin-top: 2px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .time {
    font-size: var(--fs-sm);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .more {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-2);
    opacity: 0;
    transition: opacity var(--t-fast) var(--ease), background var(--t-fast) var(--ease);
  }
  .row:hover .more,
  .row:focus-within .more {
    opacity: 1;
  }
  .more:hover {
    background: var(--hover);
    color: var(--text);
  }
  .eq {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: flex-end;
    justify-content: center;
    gap: 2px;
    padding-bottom: 12px;
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.45);
  }
  .eq i {
    width: 3px;
    height: 14px;
    border-radius: 2px;
    background: #fff;
    transform-origin: bottom;
    animation: bounce 0.9s ease-in-out infinite;
  }
  .eq i:nth-child(2) {
    animation-delay: -0.3s;
  }
  .eq i:nth-child(3) {
    animation-delay: -0.6s;
  }
  @keyframes bounce {
    0%,
    100% {
      transform: scaleY(0.3);
    }
    50% {
      transform: scaleY(1);
    }
  }
</style>
