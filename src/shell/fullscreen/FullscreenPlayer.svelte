<!-- Fullscreen player: the cover big on the song's own colors, the controls, and the synced
     lyrics (or the lyrics editor) beside it. -->
<script lang="ts">
  import { fade, fly, scale } from "svelte/transition";
  import { api, coverAt, formatTime, proxiedCover, type Track } from "../../lib/api";
  import Backdrop from "../../lib/backdrop/Backdrop.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import IconButton from "../../lib/components/IconButton.svelte";
  import LikeButton from "../../lib/components/LikeButton.svelte";
  import type { Rgb } from "../../lib/backdrop/colors";
  import SeekBar from "../../lib/components/SeekBar.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { history } from "../../lib/stores/history.svelte";
  import { settings } from "../../lib/stores/settings.svelte";
  import RangePanel from "../RangePanel.svelte";
  import Visualizer from "./Visualizer.svelte";
  import Spinner from "../../lib/components/Spinner.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { openArtist } from "../../lib/navigate";
  import { offline } from "../../lib/stores/offline.svelte";
  import { player } from "../../lib/stores/player.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import WindowControls from "../WindowControls.svelte";
  import LyricsEditor from "./LyricsEditor.svelte";
  import LyricsView from "./LyricsView.svelte";

  const track = $derived(player.current?.track ?? null);
  const cover = $derived(track?.cover ?? player.via?.cover ?? null);
  let hd = $state<{ id: string; url: string } | null>(null);
  function artUrl(tr: Track | null): string | null {
    const url = tr && hd?.id === tr.id ? hd.url : coverAt(tr?.cover ?? player.via?.cover ?? null, 1200);
    return url ? proxiedCover(url) : null;
  }
  let editing = $state(false);
  let colors = $state<Rgb[]>([]);

  $effect(() => {
    const tr = track;
    if (!tr || !offline.online) return;
    api
      .coverHd(tr)
      .then((url) => {
        if (url && player.current?.track.id === tr.id) hd = { id: tr.id, url };
      })
      .catch(() => {});
  });

  function close() {
    ui.fullscreenOpen = false;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && !editing && !e.defaultPrevented) close();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="full" role="dialog" aria-modal="true" aria-label={t("full.title")} transition:fly={{ y: 24, duration: 220 }}>
  <Backdrop cover={cover} place="fullscreen" bind:colors />
  {#if settings.values.visualizer && track}<Visualizer {colors} style={settings.values.visualizerStyle} />{/if}
  <div class="top" data-tauri-drag-region>
    <IconButton icon="chevronDown" label={t("full.close")} size={20} onclick={close} />
    <WindowControls />
  </div>

  <div class="body">
    <section class="now">
      <div class="art">
        {#if player.markers}<RangePanel docked />{/if}
        {#each [track] as tr (tr?.id)}
          {@const url = artUrl(tr)}
          <div class="art-cell" in:scale={{ start: 0.94, duration: 520, opacity: 0 }} out:fade={{ duration: 360 }}>
            {#if url}
              <img src={url} alt="" draggable="false" />
            {:else}
              <div class="art-empty"><Icon name="music" size={64} /></div>
            {/if}
          </div>
        {/each}
      </div>

      {#if track}
        <div class="meta">
          {#each [track] as tr (tr.id)}
            <div class="names" in:fly={{ y: 6, duration: 260, delay: 80 }} out:fade={{ duration: 120 }}>
              <h1 class="truncate">{tr.title}</h1>
              <p class="truncate">
                {#each tr.artists as a, i (i)}{#if i > 0}{", "}{/if}<button
                    class="artist"
                    onclick={() => {
                      close();
                      openArtist(a, tr.source);
                    }}>{a.name}</button
                  >{/each}
                {#if history.plays(tr) > 0}<span class="plays"> · {t("plays.count", { n: history.plays(tr) })}</span>{/if}
              </p>
            </div>
          {/each}
          <div class="tools">
            <IconButton
              icon="range"
              label={player.markers ? t("range.close") : t("range.open")}
              active={!!player.markers}
              disabled={!track || player.durationMs <= 0}
              onclick={() => (player.markers ? player.hideMarkers() : player.showMarkers())} />
            <IconButton icon="layers" label={t("versions.title")} disabled={!track} onclick={() => track && dialogs.open({ kind: "versions", track })} />
            <IconButton
              icon="audioLines"
              label={settings.values.visualizer ? t("viz.hide") : t("viz.show")}
              active={settings.values.visualizer}
              onclick={() => settings.set("visualizer", !settings.values.visualizer)} />
          </div>
          <LikeButton {track} size={22} />
        </div>
      {/if}

      <div class="seek">
        <SeekBar tall />
        <div class="times">
          <span>{formatTime(player.positionMs)}</span>
          <span>{formatTime(player.durationMs || null)}</span>
        </div>
      </div>

      <div class="transport">
        <IconButton icon="shuffle" label={t("player.shuffle")} active={player.queue.shuffle} onclick={() => player.toggleShuffle()} />
        <IconButton icon="prev" label={t("player.previous")} size={22} disabled={!track} onclick={() => player.previous()} />
        <button class="play" aria-label={player.playing ? t("player.pause") : t("player.play")} disabled={!track} onclick={() => player.toggle()}>
          {#if player.loading || player.buffering}<Spinner size={22} />{:else}<Icon name={player.playing ? "pause" : "play"} size={22} />{/if}
        </button>
        <IconButton icon="next" label={t("player.next")} size={22} disabled={!track} onclick={() => player.next()} />
        <IconButton
          icon={player.queue.repeat === "one" ? "repeatOne" : "repeat"}
          label={player.queue.repeat === "one" ? t("player.repeatOne") : player.queue.repeat === "all" ? t("player.repeatAll") : t("player.repeatOff")}
          active={player.queue.repeat !== "off"}
          onclick={() => player.cycleRepeat()} />
      </div>

    </section>

    <section class="side">
      {#if editing}
        <div class="pane" in:fade={{ duration: 160 }}><LyricsEditor onClose={() => (editing = false)} /></div>
      {:else}
        <div class="pane" in:fade={{ duration: 160 }}><LyricsView onEdit={() => (editing = true)} /></div>
      {/if}
    </section>
  </div>
</div>

<style>
  .full {
    position: fixed;
    inset: 0;
    z-index: 50;
    overflow: hidden;
    background: var(--bg);
  }
  .top {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    z-index: 3;
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 52px;
    padding: 0 12px;
  }
  .body {
    position: relative;
    z-index: 1;
    display: grid;
    grid-template-columns: minmax(300px, 0.85fr) minmax(0, 1.15fr);
    gap: clamp(32px, 5vw, 88px);
    height: 100%;
    padding: 64px clamp(32px, 5vw, 80px) 40px;
  }
  .now {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 22px;
    min-width: 0;
    max-width: 480px;
    justify-self: end;
    width: 100%;
  }
  .art {
    position: relative;
    display: grid;
    width: 100%;
    aspect-ratio: 1;
  }
  .art-cell {
    grid-area: 1 / 1;
    overflow: hidden;
    border-radius: 12px;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.42);
  }
  .art-cell img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .art-empty {
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-3);
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .names {
    flex: 1;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .meta p {
    margin: 4px 0 0;
    font-size: var(--fs-lg);
    color: var(--text-2);
  }
  .artist {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    color: inherit;
  }
  .artist:hover {
    color: var(--text);
    text-decoration: underline;
    text-underline-offset: 3px;
  }
  .plays {
    color: var(--text-3);
  }
  .times {
    display: flex;
    justify-content: space-between;
    margin-top: 6px;
    font-size: var(--fs-xs);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .tools {
    display: flex;
    gap: 2px;
  }
  .transport {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 14px;
  }
  .play {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    border: 0;
    border-radius: 50%;
    background: var(--primary);
    color: var(--on-primary);
    transition: transform var(--t-fast) var(--ease), background var(--t-fast) var(--ease);
  }
  .play:hover:not(:disabled) {
    background: #fff;
    transform: scale(1.04);
  }
  .play:active:not(:disabled) {
    transform: scale(0.96);
  }
  .play:disabled {
    opacity: 0.4;
  }
  .play :global(.spinner) {
    color: var(--on-primary);
  }
  .side {
    display: grid;
    min-width: 0;
    min-height: 0;
  }
  .pane {
    grid-area: 1 / 1;
    min-height: 0;
  }
</style>
