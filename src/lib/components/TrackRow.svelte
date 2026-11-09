<script lang="ts">
  import { formatTime, type Track } from "../api";
  import { t } from "../i18n/index.svelte";
  import { openArtist, playsFrom } from "../navigate";
  import { offline } from "../stores/offline.svelte";
  import { player } from "../stores/player.svelte";
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";
  import LikeButton from "./LikeButton.svelte";
  import SourceBadge from "./SourceBadge.svelte";

  let {
    track,
    selected = false,
    compact = false,
    showAlbum = false,
    hidden = false,
    onactivate,
    onselect,
    onmenu,
    onpress,
  }: {
    track: Track;
    selected?: boolean;
    compact?: boolean;
    showAlbum?: boolean;
    /** Marked as "Don't play": shown dimmed. */
    hidden?: boolean;
    onactivate: () => void;
    onselect?: (e: MouseEvent) => void;
    onmenu: (e: MouseEvent) => void;
    onpress?: (e: PointerEvent) => void;
  } = $props();

  const isCurrent = $derived(player.current?.track.id === track.id);
  const unmatched = $derived(track.via === "none");
  const unavailable = $derived(!offline.online && !offline.isAvailable(track));
  const downloaded = $derived(track.source !== "local" && offline.downloaded.has(track.id));

  function onKey(e: KeyboardEvent) {
    if (e.key === "Enter") onactivate();
  }

  function artistClick(e: MouseEvent, i: number) {
    e.stopPropagation();
    openArtist(track.artists[i], track.source);
  }
</script>

<div
  class="row"
  class:current={isCurrent}
  class:selected
  class:compact
  class:dim={unmatched || hidden || unavailable}
  class:with-album={showAlbum}
  role="button"
  tabindex="0"
  aria-pressed={selected}
  onclick={(e) => onselect?.(e)}
  ondblclick={onactivate}
  onkeydown={onKey}
  onpointerdown={(e) => onpress?.(e)}
  oncontextmenu={onmenu}>
  <div class="art">
    <Cover src={track.cover} size={compact ? 40 : 44} />
    {#if isCurrent && player.playing}
      <span class="eq" aria-hidden="true"><i></i><i></i><i></i></span>
    {/if}
  </div>
  <div class="text">
    <div class="title truncate">{track.title}</div>
    <div class="artist">
      <SourceBadge source={playsFrom(track)} />
      {#if downloaded}<span class="saved" title={t("offline.downloaded")}><Icon name="download" size={12} /></span>{/if}
      <span class="truncate">
        {#each track.artists as a, i (i)}{#if i > 0}, {/if}<button class="link" tabindex="-1" onclick={(e) => artistClick(e, i)}>{a.name}</button>{/each}
        {#if unmatched}<span class="note"> · {t("track.noMatch")}</span>{/if}
      </span>
    </div>
  </div>
  {#if showAlbum}<span class="album truncate">{track.album?.name ?? ""}</span>{/if}
  <LikeButton {track} quiet />
  <span class="time">{formatTime(track.durationMs)}</span>
  <button class="more" aria-label={t("menu.more")} onclick={(e) => (e.stopPropagation(), onmenu(e))}>
    <Icon name="more" size={18} />
  </button>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto auto;
    align-items: center;
    gap: var(--s3);
    height: 58px;
    padding: 0 var(--s2) 0 8px;
    border-radius: var(--r-md);
    outline: none;
    transition: background var(--t-fast) var(--ease);
    /* Rows off screen skip layout and paint, which keeps long lists smooth. */
    content-visibility: auto;
    contain-intrinsic-size: auto 58px;
  }
  .row.with-album {
    grid-template-columns: auto minmax(0, 1.4fr) minmax(0, 1fr) auto auto auto;
  }
  .row:hover,
  .row:focus-visible {
    background: var(--hover);
  }
  .row.selected {
    background: var(--selected);
  }
  .compact {
    height: 50px;
    padding: 0 6px;
    gap: 10px;
    contain-intrinsic-size: auto 50px;
  }
  .dim .text,
  .dim .art {
    opacity: 0.5;
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
  .link {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }
  .link:hover {
    color: var(--text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .note {
    color: var(--text-3);
  }
  .saved {
    flex: none;
    display: inline-grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: rgba(255, 255, 255, 0.14);
    color: var(--text);
  }
  .album {
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .time {
    min-width: 38px;
    text-align: right;
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
  .row:focus-within .more,
  .row.selected .more {
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
  @media (max-width: 1100px) {
    .row.with-album {
      grid-template-columns: auto minmax(0, 1fr) auto auto auto;
    }
    .album {
      display: none;
    }
  }
</style>
