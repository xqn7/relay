<script lang="ts">
  import { api, artistLine, formatLength, type AlbumPage, type SearchSource } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import SearchBox from "../lib/components/SearchBox.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import TrackList from "../lib/components/TrackList.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { openArtist } from "../lib/navigate";
  import { player } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import { fold, matchesQuery } from "../lib/text";
  import { errorText } from "../shell/dialogs/importText";
  import EmptyState from "./EmptyState.svelte";
  import ListHeader from "./ListHeader.svelte";

  let { source, id }: { source: SearchSource; id: string } = $props();
  let album = $state<AlbumPage | null>(null);
  let error = $state("");
  let query = $state("");

  async function load() {
    error = "";
    try {
      album = await api.albumPage(source, id);
      ui.heroCover = album.cover;
    } catch (e) {
      error = errorText(e);
    }
  }

  $effect(() => {
    void id;
    album = null;
    load();
    return () => (ui.heroCover = null);
  });

  const q = $derived(fold(query.trim()));
  const tracks = $derived((album?.tracks ?? []).filter((x) => matchesQuery(`${x.title} ${artistLine(x)}`, q)));
  const total = $derived((album?.tracks ?? []).reduce((s, x) => s + (x.durationMs ?? 0), 0));
  const kindLabel = $derived(album?.kind === "single" ? t("album.single") : album?.kind === "ep" ? t("album.ep") : t("album.kind"));

  function play(shuffle: boolean) {
    if (!album?.tracks.length) return;
    if (player.queue.shuffle !== shuffle) player.toggleShuffle();
    player.playTracks(album.tracks, shuffle ? Math.floor(Math.random() * album.tracks.length) : 0);
  }
</script>

{#if error}
  <EmptyState icon="alert" title={error}>
    <Button variant="secondary" icon="refresh" onclick={load}>{t("dialog.retry")}</Button>
  </EmptyState>
{:else if !album}
  <div class="center"><Spinner size={22} /></div>
{:else}
  <ListHeader
    kind={kindLabel}
    title={album.title}
    cover={album.cover}
    icon="disc"
    chips={[{ text: `${t("library.songs", { n: album.tracks.length })} · ${formatLength(total, t("time.hourShort"))}`, icon: "music" }]}>
    {#snippet subtitle()}
      {#each album!.artists as a, i (i)}{#if i > 0}, {/if}<button class="link" onclick={() => openArtist(a, source)}>{a.name}</button>{/each}{#if album!.year}<span> · {album!.year}</span>{/if}
    {/snippet}
    {#snippet actions()}
      <Button variant="primary" icon="play" disabled={!album!.tracks.length} onclick={() => play(false)}>{t("playlist.playAll")}</Button>
      <IconButton icon="shuffle" label={t("playlist.shuffle")} variant="solid" onclick={() => play(true)} />
      {#if album!.link}<IconButton icon="external" label={t("album.openOnline")} variant="solid" onclick={() => api.openExternal(album!.link!)} />{/if}
    {/snippet}
    {#snippet tools()}
      <SearchBox bind:value={query} placeholder={t("album.search")} />
    {/snippet}
  </ListHeader>
  {#if tracks.length}
    <TrackList {tracks} />
  {:else}
    <div class="none">{t("list.noMatches")}</div>
  {/if}
{/if}

<style>
  .center {
    display: grid;
    place-items: center;
    height: 50vh;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    color: var(--text);
    font-weight: 600;
  }
  .link:hover {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .none {
    padding: 40px 0;
    text-align: center;
    color: var(--text-2);
    font-size: var(--fs-sm);
  }
</style>
