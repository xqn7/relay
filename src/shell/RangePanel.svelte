<!-- The A–B tools above the player bar while the markers are shown: fine-tune A and B, then
     loop that part, preview it, save it as the song's trim, or export it as an MP3 clip. -->
<script lang="ts">
  import { fly } from "svelte/transition";
  import Button from "../lib/components/Button.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import { exportSongs } from "../lib/exportActions";
  import { t } from "../lib/i18n/index.svelte";
  import { library } from "../lib/stores/library.svelte";
  import { player } from "../lib/stores/player.svelte";

  /** `docked`: over the bottom of the fullscreen player's cover, instead of floating over the player bar. */
  let { docked = false }: { docked?: boolean } = $props();

  const marks = $derived(player.markers!);
  const track = $derived(player.current?.track ?? null);
  const trimmed = $derived(!!track && !!library.trims[track.id]);

  /** "1:02.3": tenths, since the markers move in tenths. */
  function clock(ms: number): string {
    const s = Math.max(0, ms) / 1000;
    const m = Math.floor(s / 60);
    return `${m}:${(s - m * 60).toFixed(1).padStart(4, "0")}`;
  }

  /** 0.1 s per click, 1 s with Shift. */
  function nudge(which: "a" | "b", dir: number, e: MouseEvent) {
    const step = (e.shiftKey ? 1000 : 100) * dir;
    if (which === "a") player.setMarkers(marks.aMs + step, marks.bMs);
    else player.setMarkers(marks.aMs, marks.bMs + step);
  }

  function setHere(which: "a" | "b") {
    const pos = player.positionMs;
    if (which === "a") player.setMarkers(pos, Math.max(marks.bMs, pos + 1000));
    else player.setMarkers(Math.min(marks.aMs, pos - 1000), pos);
  }

  function exportClip() {
    if (track) exportSongs([track], { startMs: marks.aMs, endMs: marks.bMs });
  }
</script>

<div class="range-panel glass" class:docked role="toolbar" aria-label={t("range.title")} transition:fly={{ y: 8, duration: 160 }}>
  {#each ["a", "b"] as const as which (which)}
    <div class="point">
      <span class="letter">{which.toUpperCase()}</span>
      <IconButton icon="minus" size={14} label={t("range.earlier")} onclick={(e) => nudge(which, -1, e)} />
      <span class="time">{clock(which === "a" ? marks.aMs : marks.bMs)}</span>
      <IconButton icon="plus" size={14} label={t("range.later")} onclick={(e) => nudge(which, 1, e)} />
      <button class="here" title={t(which === "a" ? "range.setAHint" : "range.setBHint")} onclick={() => setHere(which)}>{t("range.here")}</button>
    </div>
  {/each}
  <span class="sep"></span>
  <Button small variant={player.looping ? "primary" : "ghost"} onclick={() => player.toggleLoop()}>{t("range.loop")}</Button>
  <Button small variant="ghost" onclick={() => player.previewMarked()}>{t("range.preview")}</Button>
  <Button small variant="ghost" onclick={() => player.saveTrim()}>{t("range.saveTrim")}</Button>
  {#if trimmed && track}
    <Button
      small
      variant="ghost"
      onclick={() => {
        player.resetTrim(track);
        player.hideMarkers();
      }}>{t("trim.reset")}</Button>
  {/if}
  <Button small variant="ghost" onclick={exportClip}>{t("range.exportClip")}</Button>
  <IconButton icon="close" size={16} label={t("range.close")} onclick={() => player.hideMarkers()} />
</div>

<style>
  .range-panel {
    position: fixed;
    left: calc(var(--sidebar-w) + 12px + 50%);
    transform: translateX(calc(-50% - (var(--sidebar-w) + 22px) / 2));
    bottom: calc(var(--player-h) + 26px);
    z-index: 60;
    display: flex;
    align-items: center;
    gap: var(--s2);
    padding: 6px 8px 6px 12px;
    border-radius: var(--r-lg);
    white-space: nowrap;
  }
  .docked {
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: 10px;
    transform: none;
    z-index: 2;
    flex-wrap: wrap;
    justify-content: center;
    row-gap: 4px;
    padding: 8px;
    white-space: normal;
  }
  .docked .sep {
    display: none;
  }
  .point {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .point + .point {
    margin-left: var(--s2);
  }
  .letter {
    width: 16px;
    font-size: var(--fs-xs);
    font-weight: 700;
    color: var(--text-2);
  }
  .time {
    min-width: 48px;
    text-align: center;
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
  }
  .here {
    margin-left: 2px;
    padding: 3px 7px;
    border: 0;
    border-radius: var(--r-sm);
    background: rgba(255, 255, 255, 0.08);
    color: var(--text-2);
    font: inherit;
    font-size: var(--fs-xs);
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease);
  }
  .here:hover {
    background: rgba(255, 255, 255, 0.14);
    color: var(--text);
  }
  .sep {
    width: 1px;
    height: 20px;
    margin: 0 var(--s1);
    background: var(--hairline);
  }
</style>
