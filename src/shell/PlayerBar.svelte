<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { formatTime } from "../lib/api";
  import Cover from "../lib/components/Cover.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import LikeButton from "../lib/components/LikeButton.svelte";
  import { openArtist, playsFrom } from "../lib/navigate";
  import Slider from "../lib/components/Slider.svelte";
  import SourceBadge from "../lib/components/SourceBadge.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { player } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import { openTrackMenu } from "../lib/trackMenu";

  const track = $derived(player.current?.track ?? null);
  let seekPreview = $state<number | null>(null);
  const progress = $derived(seekPreview ?? (player.durationMs > 0 ? player.positionMs / player.durationMs : 0));
  const repeatLabel = $derived(
    player.queue.repeat === "one" ? t("player.repeatOne") : player.queue.repeat === "all" ? t("player.repeatAll") : t("player.repeatOff"),
  );
  const volumeIcon = $derived(player.muted || player.volume === 0 ? "volumeOff" : player.volume < 0.5 ? "volumeLow" : "volume");
  let volumeOpen = $state(false);

  function wheelVolume(e: WheelEvent) {
    e.preventDefault();
    player.setVolume(player.volume + (e.deltaY < 0 ? 0.05 : -0.05));
  }
</script>

<footer class="bar glass">
  {#if track}
    <div class="seek">
      <Slider
        thin
        label={t("player.seek")}
        value={progress}
        valueText={formatTime(player.positionMs)}
        oninput={(v) => (seekPreview = v)}
        onchange={(v) => {
          seekPreview = null;
          player.seek(v * player.durationMs);
        }} />
    </div>
  {/if}

  <div class="now">
    {#if track}
      <!-- Old and new song share one cell, so the change is a soft crossfade, not a jump. A keyed
           each keeps the outgoing song's own details while it fades. -->
      <div class="swap">
        {#each [track] as tr (tr.id)}
          <div class="swap-cell" in:scale={{ start: 0.96, duration: 280, opacity: 0 }} out:fade={{ duration: 160 }}>
            <button class="cover-btn" title={t("full.open")} onclick={() => (ui.fullscreenOpen = true)}><Cover src={tr.cover ?? player.via?.cover ?? null} size={44} /></button>
            <div class="meta" role="button" tabindex="-1" oncontextmenu={(e) => openTrackMenu(e, { tracks: [tr], index: 0 })}>
              <div class="title truncate">{tr.title}</div>
              <div class="artist">
                <SourceBadge source={player.via?.source ?? playsFrom(tr)} />
                <span class="truncate">
                  {#each tr.artists as a, i (i)}{#if i > 0}, {/if}<button class="artist-link" onclick={() => openArtist(a, tr.source)}>{a.name}</button>{/each}
                </span>
              </div>
            </div>
          </div>
        {/each}
      </div>
      <LikeButton {track} />
    {:else}
      <div class="cover-empty"><Icon name="music" size={18} /></div>
      <div class="meta"><div class="title muted">{t("player.nothing")}</div></div>
    {/if}
  </div>

  <div class="transport">
    <span class="time">{track ? formatTime(seekPreview != null ? seekPreview * player.durationMs : player.positionMs) : ""}</span>
    <IconButton icon="prev" label={t("player.previous")} size={18} disabled={!track} onclick={() => player.previous()} />
    {#if player.loading || player.buffering}
      <button class="play" aria-label={t("player.loading")} onclick={() => player.toggle()}><Spinner size={18} /></button>
    {:else}
      <button class="play" aria-label={player.playing ? t("player.pause") : t("player.play")} disabled={!track} onclick={() => player.toggle()}>
        <Icon name={player.playing ? "pause" : "play"} size={18} />
      </button>
    {/if}
    <IconButton icon="next" label={t("player.next")} size={18} disabled={!track} onclick={() => player.next()} />
    <span class="time">{track ? formatTime(player.durationMs || null) : ""}</span>
  </div>

  <div class="tools">
    <IconButton icon="shuffle" label={t("player.shuffle")} active={player.queue.shuffle} onclick={() => player.toggleShuffle()} />
    <IconButton
      icon={player.queue.repeat === "one" ? "repeatOne" : "repeat"}
      label={repeatLabel}
      active={player.queue.repeat !== "off"}
      onclick={() => player.cycleRepeat()} />
    <div class="volume" role="group" onmouseenter={() => (volumeOpen = true)} onmouseleave={() => (volumeOpen = false)} onwheel={wheelVolume}>
      {#if volumeOpen}
        <div class="volume-pop">
          <Slider label={t("player.volume")} value={player.muted ? 0 : player.volume} oninput={(v) => player.setVolume(v)} onchange={(v) => player.setVolume(v)} />
        </div>
      {/if}
      <IconButton icon={volumeIcon} label={player.muted ? t("player.unmute") : t("player.mute")} onclick={() => player.toggleMute()} />
    </div>
    <IconButton icon="micVocal" label={t("full.lyrics")} active={ui.fullscreenOpen} disabled={!track} onclick={() => (ui.fullscreenOpen = !ui.fullscreenOpen)} />
    <IconButton icon="queue" label={t("player.queue")} active={ui.queueOpen} onclick={() => (ui.queueOpen = !ui.queueOpen)} />
  </div>
</footer>

<style>
  .bar {
    position: fixed;
    left: calc(var(--sidebar-w) + 12px);
    right: 10px;
    bottom: 10px;
    height: var(--player-h);
    z-index: 40;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
    align-items: center;
    gap: var(--s4);
    padding: 0 var(--s3);
    border-radius: var(--r-lg);
    background: rgba(20, 20, 22, 0.55);
  }
  .seek {
    position: absolute;
    left: 14px;
    right: 14px;
    top: -9px;
  }
  .now {
    display: flex;
    align-items: center;
    gap: var(--s3);
    min-width: 0;
  }
  .swap {
    display: grid;
    min-width: 0;
  }
  .swap-cell {
    grid-area: 1 / 1;
    display: flex;
    align-items: center;
    gap: var(--s3);
    min-width: 0;
  }
  .cover-btn {
    flex: none;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: none;
    transition: transform var(--t-fast) var(--ease);
  }
  .cover-btn:hover {
    transform: scale(1.04);
  }
  .meta {
    min-width: 0;
    outline: none;
  }
  .title {
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .artist {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
    font-size: var(--fs-sm);
    color: var(--text-2);
    min-width: 0;
  }
  .cover-empty {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: var(--text-3);
  }
  .transport {
    display: flex;
    align-items: center;
    gap: var(--s2);
  }
  .time {
    width: 44px;
    font-size: var(--fs-xs);
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
    text-align: center;
  }
  .play {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    margin: 0 4px;
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
  .tools {
    display: flex;
    justify-content: flex-end;
    gap: 2px;
  }
  .volume {
    position: relative;
  }
  .volume-pop {
    position: absolute;
    right: -8px;
    bottom: 44px;
    width: 140px;
    padding: 10px 14px;
    border-radius: var(--r-md);
    border: 1px solid var(--hairline);
    /* Solid, not blurred: a blur nested inside the bar's own blur renders with artifacts. */
    background: rgb(28, 28, 31);
  }
  .volume-pop::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: -16px;
    height: 16px;
  }
  .artist-link {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    color: inherit;
  }
  .artist-link:hover {
    color: var(--text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
</style>
