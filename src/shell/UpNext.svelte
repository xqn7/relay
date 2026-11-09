<!-- "Up next": a small card near the player bar in the last seconds of a song, with the next
     song and a way to skip it before it starts. -->
<script lang="ts">
  import { fly } from "svelte/transition";
  import { artistLine } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Cover from "../lib/components/Cover.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { player } from "../lib/stores/player.svelte";
  import { settings } from "../lib/stores/settings.svelte";

  const SHOW_BEFORE_END_MS = 10_000;
  /** Short songs would show the card for a large part of their length. */
  const MIN_SONG_MS = 45_000;

  const remaining = $derived(player.durationMs - player.positionMs);
  const next = $derived(player.nextItem);
  const shown = $derived(
    settings.values.upNext &&
      player.playing &&
      !player.loading &&
      player.durationMs >= MIN_SONG_MS &&
      remaining > 0 &&
      // With crossfade the next song takes over that much earlier.
      remaining <= SHOW_BEFORE_END_MS + settings.values.crossfade * 1000 &&
      next != null,
  );
</script>

{#if shown && next}
  <div class="card glass" transition:fly={{ y: 10, duration: 200 }}>
    <Cover src={next.track.cover} size={40} />
    <div class="text">
      <div class="label">{t("upNext.label")}</div>
      <div class="title truncate">{next.track.title}</div>
      <div class="artist truncate">{artistLine(next.track)}</div>
    </div>
    <Button variant="secondary" onclick={() => player.remove(next.uid)}>{t("upNext.skip")}</Button>
  </div>
{/if}

<style>
  .card {
    position: fixed;
    right: 18px;
    bottom: calc(var(--player-h) + 22px);
    z-index: 45;
    display: flex;
    align-items: center;
    gap: 12px;
    width: 340px;
    padding: 10px 10px 10px 10px;
    border-radius: var(--r-lg);
    background: rgba(20, 20, 22, 0.6);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .label {
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-3);
  }
  .title {
    margin-top: 1px;
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .artist {
    font-size: var(--fs-xs);
    color: var(--text-2);
  }
</style>
