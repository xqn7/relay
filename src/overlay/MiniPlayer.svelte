<!-- The floating mini player: a slim pill at the top of the screen with the song (cover, title,
     artist) on a lighter glass segment and like / previous / play-pause / next on a darker one.
     It only shows the main window's player and asks it to act; it plays nothing itself and
     does no work while idle (no timers, no animation loops). -->
<script lang="ts">
  import { emit, listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import BackgroundImage from "../lib/backdrop/BackgroundImage.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import LikeButton from "../lib/components/LikeButton.svelte";
  import { NOW_EVENT, READY_EVENT, REMOTE_EVENT, type NowFeed, type RemoteAction } from "../lib/desktop/feed";
  import { t } from "../lib/i18n/index.svelte";

  /** Windows 11 blurs what's behind the window itself; Windows 10 gets a denser fill. */
  const osBlur = new URLSearchParams(location.search).has("blur");
  /** A press that moves further than this is a drag, not a click. */
  const DRAG_PX = 4;

  let now = $state<NowFeed | null>(null);
  let liked = $state(false);
  const track = $derived(now?.track ?? null);
  const quick = $derived(now?.quality === "low");

  function remote(action: RemoteAction) {
    emit(REMOTE_EVENT, action).catch(() => {});
  }

  function like() {
    liked = !liked;
    remote("like");
  }

  // Click on the song opens Relay; pressing and moving drags the pill.
  let press: { x: number; y: number; onSong: boolean } | null = null;

  function onDown(e: PointerEvent) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    press = { x: e.screenX, y: e.screenY, onSong: !!(e.target as HTMLElement).closest(".song") };
  }

  function onMove(e: PointerEvent) {
    if (!press || Math.hypot(e.screenX - press.x, e.screenY - press.y) < DRAG_PX) return;
    press = null;
    getCurrentWindow().startDragging().catch(() => {});
  }

  function onUp() {
    if (press?.onSong) invoke("main_window_show").catch(() => {});
    press = null;
  }

  onMount(() => {
    const off = listen<NowFeed>(NOW_EVENT, ({ payload }) => {
      now = payload;
      liked = payload.liked;
    });
    off.then(() => emit(READY_EVENT));
    return () => off.then((f) => f());
  });
</script>

<div class="pill" class:solid={!osBlur} role="presentation" onpointerdown={onDown} onpointermove={onMove} onpointerup={onUp}>
  {#if now?.background}
    <div class="image"><BackgroundImage url={now.background.url} look={now.background.look} still={quick} /></div>
  {/if}
  <div class="song" title={t("mini.open")}>
    {#key track?.id}
      <div class="song-inner" in:fade={{ duration: quick ? 80 : 220 }}>
        {#if track?.cover}
          <img class="cover" src={track.cover} alt="" draggable="false" />
        {:else}
          <span class="cover empty"><Icon name="music" size={14} /></span>
        {/if}
        <span class="text">
          <span class="title truncate">{track?.title ?? t("mini.nothing")}</span>
          {#if track}<span class="artist truncate">{track.artist}</span>{/if}
        </span>
      </div>
    {/key}
  </div>
  <div class="controls">
    {#if track}
      <LikeButton track={{ id: track.id }} size={15} {liked} ontoggle={like} />
    {/if}
    <button class="ctl" aria-label={t("player.previous")} title={t("player.previous")} disabled={!track} onclick={() => remote("previous")}>
      <Icon name="prev" size={15} />
    </button>
    <button class="play" aria-label={now?.playing ? t("player.pause") : t("player.play")} disabled={!track} onclick={() => remote("toggle")}>
      <Icon name={now?.playing ? "pause" : "play"} size={13} />
    </button>
    <button class="ctl" aria-label={t("player.next")} title={t("player.next")} disabled={!track} onclick={() => remote("next")}>
      <Icon name="next" size={15} />
    </button>
  </div>
</div>

<style>
  :global(html.overlay #app) {
    height: 100vh;
  }
  .pill {
    position: relative;
    display: flex;
    height: 100vh;
    overflow: hidden;
    /* Windows 11 rounds the window itself to 8 px; the pill follows it. */
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(18, 18, 20, 0.42);
  }
  .solid {
    border-radius: 999px;
    background: rgba(18, 18, 20, 0.9);
  }
  .image {
    position: absolute;
    inset: 0;
    opacity: 0.55;
    pointer-events: none;
  }
  .song {
    position: relative;
    flex: 1;
    min-width: 0;
    background: rgba(255, 255, 255, 0.07);
  }
  .song-inner {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 100%;
    padding: 0 8px 0 6px;
  }
  .solid .song-inner {
    padding-left: 8px;
  }
  .cover {
    flex: none;
    width: 28px;
    height: 28px;
    border-radius: 5px;
    object-fit: cover;
    background: rgba(255, 255, 255, 0.06);
  }
  .empty {
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .text {
    display: grid;
    min-width: 0;
    line-height: 1.2;
  }
  .title {
    font-size: 12px;
    font-weight: 600;
  }
  .artist {
    font-size: 11px;
    color: var(--text-2);
  }
  .controls {
    position: relative;
    display: flex;
    align-items: center;
    gap: 1px;
    padding: 0 6px 0 2px;
    background: rgba(0, 0, 0, 0.22);
  }
  .controls :global(.like) {
    width: 24px;
    height: 24px;
  }
  .ctl {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    transition: color var(--t-fast) var(--ease);
  }
  .ctl:hover:not(:disabled) {
    color: var(--text);
  }
  .play {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    margin: 0 2px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: var(--primary);
    color: #0a0a0b;
    transition: transform var(--t-fast) var(--ease);
  }
  .play:active:not(:disabled) {
    transform: scale(0.94);
  }
  button:disabled {
    opacity: 0.4;
  }
</style>
