<!-- Desktop lyrics: the current line floating over the desktop and games like a subtitle, the
     next line faint below it. Locked (normal) it lets every click through; unlocked it shows a
     frame to drag and resize, and the text grows with the window. -->
<script lang="ts">
  import { emit, listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount, tick } from "svelte";
  import Icon from "../lib/components/Icon.svelte";
  import { LYRICS_EVENT, NOW_EVENT, READY_EVENT, positionNow, type LyricsFeed, type NowFeed } from "../lib/desktop/feed";
  import { t } from "../lib/i18n/index.svelte";
  import { activeIndex } from "../lib/lyrics/lines";
  import LyricText from "../lib/lyrics/LyricText.svelte";

  let now = $state<NowFeed | null>(null);
  let feed = $state<LyricsFeed | null>(null);
  let pos = $state(0);

  // Only lyrics of the song that's playing count (the feed can lag a song change).
  const lines = $derived(feed && now?.track && feed.trackId === now.track.id ? feed.lines : []);
  const index = $derived(activeIndex(lines, pos));
  const current = $derived(index >= 0 ? lines[index] : null);
  const next = $derived(lines[index + 1] ?? null);
  const unlocked = $derived(feed?.unlocked ?? false);

  // Follows the music every frame only while it plays; paused, it just takes reported jumps.
  $effect(() => {
    const n = now;
    if (!n) return;
    pos = positionNow(n);
    if (!n.playing) return;
    let raf = 0;
    const tick = () => {
      pos = positionNow(n);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  /** A line wider than the window gets smaller type instead of losing its last words. */
  const SMALLEST = 0.6;

  function fit(el: HTMLElement, _text: string) {
    const apply = () => {
      el.style.fontSize = "";
      const least = parseFloat(getComputedStyle(el).fontSize) * SMALLEST;
      let size = least / SMALLEST;
      // Text doesn't narrow exactly in step with its size, so further passes take up the rest.
      // Two pixels of slack: widths are whole numbers, and a line one pixel over still gets cut.
      for (let pass = 0; pass < 3 && size > least && el.scrollWidth > el.clientWidth; pass++) {
        size = Math.max((size * (el.clientWidth - 2)) / el.scrollWidth, least);
        el.style.fontSize = `${size}px`;
      }
    };
    apply();
    window.addEventListener("resize", apply);
    // The new line's text reaches the page after this is told about it, so measuring waits for it.
    return { update: () => void tick().then(apply), destroy: () => window.removeEventListener("resize", apply) };
  }

  function drag(e: PointerEvent) {
    if (!unlocked || e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    getCurrentWindow().startDragging().catch(() => {});
  }

  function resize(e: PointerEvent) {
    e.stopPropagation();
    getCurrentWindow().startResizeDragging("SouthEast").catch(() => {});
  }

  onMount(() => {
    const offs = [
      listen<NowFeed>(NOW_EVENT, ({ payload }) => (now = payload)),
      listen<LyricsFeed>(LYRICS_EVENT, ({ payload }) => (feed = payload)),
    ];
    Promise.all(offs).then(() => emit(READY_EVENT));
    return () => offs.forEach((o) => o.then((f) => f()));
  });
</script>

<div class="box" class:unlocked role="presentation" onpointerdown={drag}>
  {#if current}
    <p class="current" use:fit={current.text}><LyricText line={current} {pos} active still={!now?.playing} style={feed?.style ?? "karaoke"} /></p>
    {#if next && !next.gap}<p class="next" use:fit={next.text}>{next.text}</p>{/if}
  {:else if unlocked}
    <p class="hint">{t("desktopLyrics.sample")}</p>
  {/if}
  {#if unlocked}
    <div class="bar">
      <span>{t("desktopLyrics.dragHint")}</span>
      <button type="button" onclick={() => invoke("lyrics_unlock", { unlocked: false })}><Icon name="lock" size={13} />{t("desktopLyrics.lock")}</button>
    </div>
    <div class="grip" role="presentation" onpointerdown={resize}></div>
  {/if}
</div>

<style>
  :global(html.overlay #app) {
    height: 100vh;
  }
  .box {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100vh;
    padding: 0 4vw;
    text-align: center;
    border-radius: 12px;
    border: 1px solid transparent;
    /* Readable over any wallpaper or game, without a box around the text. */
    filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.85)) drop-shadow(0 0 10px rgba(0, 0, 0, 0.45));
  }
  .unlocked {
    filter: none;
    border-color: rgba(255, 255, 255, 0.18);
    background: rgba(10, 10, 11, 0.55);
    cursor: move;
  }
  p {
    margin: 0;
    max-width: 100%;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .current {
    font-size: 30vh;
    line-height: 1.25;
    color: #fff;
  }
  .next {
    font-size: 19vh;
    line-height: 1.3;
    color: rgba(255, 255, 255, 0.5);
  }
  .hint {
    font-size: 24vh;
    color: rgba(255, 255, 255, 0.7);
  }
  .bar {
    position: absolute;
    top: 6px;
    right: 8px;
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 11px;
    color: var(--text-2);
  }
  .bar button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px;
    border: 0;
    border-radius: 999px;
    background: var(--primary);
    color: #0a0a0b;
    font-size: 11px;
    font-weight: 600;
  }
  .grip {
    position: absolute;
    right: 2px;
    bottom: 2px;
    width: 14px;
    height: 14px;
    border-right: 2px solid rgba(255, 255, 255, 0.4);
    border-bottom: 2px solid rgba(255, 255, 255, 0.4);
    border-bottom-right-radius: 6px;
    cursor: nwse-resize;
  }
</style>
