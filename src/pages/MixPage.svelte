<!-- One of today's daily mixes: the songs, play / shuffle, and keep it as a playlist. -->
<script lang="ts">
  import { formatLength } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import TrackList from "../lib/components/TrackList.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { playMix, saveMix } from "../lib/mixMenu";
  import { home } from "../lib/stores/home.svelte";
  import { offline } from "../lib/stores/offline.svelte";
  import EmptyState from "./EmptyState.svelte";
  import ListHeader from "./ListHeader.svelte";

  let { number }: { number: number } = $props();

  $effect(() => {
    home.loadMixes();
  });

  const mix = $derived(home.mixes?.find((m) => m.number === number) ?? null);
  const tracks = $derived(mix ? offline.playable(mix.tracks, (x) => x) : []);
  const totalMs = $derived(mix?.tracks.reduce((sum, x) => sum + (x.durationMs ?? 0), 0) ?? 0);
</script>

{#if mix}
  <ListHeader
    kind={t("mix.kind")}
    title={t("mix.title", { n: mix.number })}
    cover={mix.tracks.find((x) => x.cover)?.cover ?? null}
    icon="radio"
    chips={[{ text: `${t("library.songs", { n: mix.tracks.length })} · ${formatLength(totalMs, t("time.hourShort"))}`, icon: "music" }]}>
    {#snippet subtitle()}{mix.artists.join(", ")}{/snippet}
    {#snippet actions()}
      <Button variant="primary" icon="play" onclick={() => playMix(mix, false)}>{t("menu.play")}</Button>
      <Button variant="secondary" icon="shuffle" onclick={() => playMix(mix, true)}>{t("playlist.shuffle")}</Button>
      <Button variant="secondary" icon="listPlus" onclick={() => saveMix(mix)}>{t("mix.save")}</Button>
    {/snippet}
  </ListHeader>
  <TrackList {tracks} showAlbum />
{:else if home.mixes === null && !home.mixesFailed}
  <div class="center"><Spinner size={22} /></div>
{:else}
  <EmptyState icon="radio" title={t("mix.gone")} body={t("mix.goneBody")} />
{/if}

<style>
  .center {
    display: grid;
    place-items: center;
    height: 50vh;
  }
</style>
