<!-- Discover: artists similar to the ones listened to most that aren't in the library yet,
     each with their top songs to preview and play. -->
<script lang="ts">
  import { untrack } from "svelte";
  import { api, type Suggestion, type Track } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Cover from "../lib/components/Cover.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import TrackList from "../lib/components/TrackList.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { home } from "../lib/stores/home.svelte";
  import { library } from "../lib/stores/library.svelte";
  import { offline } from "../lib/stores/offline.svelte";
  import { player } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import EmptyState from "./EmptyState.svelte";

  const SONGS_SHOWN = 3;
  const PARALLEL = 3;

  /** Top songs per artist ("source:id"); null while loading, [] when they couldn't be read. */
  let songs = $state<Record<string, Track[] | null>>({});

  $effect(() => {
    if (offline.online && !home.suggestions) home.loadDiscover();
  });

  const key = (s: Suggestion) => `${s.artist.source}:${s.artist.id}`;

  // Each artist's songs load a few at a time, top of the page first. Only a new list of
  // artists starts loading; the songs arriving don't.
  $effect(() => {
    const list = home.suggestions ?? [];
    return untrack(() => {
      const todo = list.filter((s) => !(key(s) in songs));
      if (!todo.length) return;
      let cancelled = false;
      const next = { ...songs };
      for (const s of todo) next[key(s)] = null;
      songs = next;
      const queue = todo.slice();
      const worker = async () => {
        for (let s = queue.shift(); s && !cancelled; s = queue.shift()) {
          const page = await api.artistPage(s.artist.source, s.artist.id).catch(() => null);
          if (!cancelled) songs = { ...songs, [key(s)]: page?.topTracks ?? [] };
        }
      };
      for (let i = 0; i < PARALLEL; i++) worker();
      return () => (cancelled = true);
    });
  });

  function openArtist(s: Suggestion) {
    ui.open({ name: "artist", source: s.artist.source, id: s.artist.id, artistName: s.artist.name });
  }

  function follow(s: Suggestion) {
    const on = !library.isFollowing(s.artist.source, s.artist.id);
    library.setFollow({ source: s.artist.source, artistId: s.artist.id, name: s.artist.name, avatar: s.artist.avatar }, on);
  }
</script>

<h1 class="page-title">{t("discover.title")}</h1>
<p class="lead">{t("discover.lead")}</p>

{#if !offline.online}
  <EmptyState icon="cloud" title={t("discover.offline")} />
{:else if home.discoverFailed}
  <EmptyState icon="alert" title={t("discover.failed")}>
    <Button variant="secondary" icon="refresh" onclick={() => home.loadDiscover()}>{t("dialog.retry")}</Button>
  </EmptyState>
{:else if !home.suggestions}
  <div class="center"><Spinner size={22} /></div>
{:else if home.suggestions.length === 0}
  <EmptyState icon="compass" title={t("discover.emptyTitle")} body={t("discover.emptyBody")} />
{:else}
  <div class="list">
    {#each home.suggestions as s (key(s))}
      {@const top = songs[key(s)]}
      <article>
        <button class="who" onclick={() => openArtist(s)}>
          <Cover src={s.artist.avatar} size={72} round fallback="user" />
          <span class="name truncate">{s.artist.name}</span>
          <span class="because truncate">{t("discover.because", { name: s.because })}</span>
        </button>
        <div class="songs">
          {#if top === null || top === undefined}
            <div class="loading"><Spinner size={16} /></div>
          {:else if top.length}
            <TrackList tracks={top.slice(0, SONGS_SHOWN)} compact warmFirst={false} />
          {:else}
            <p class="none">{t("discover.noSongs")}</p>
          {/if}
        </div>
        <div class="actions">
          <IconButton icon="play" label={t("artist.play")} variant="solid" disabled={!top?.length} onclick={() => top && player.playTracks(top)} />
          <IconButton
            icon={library.isFollowing(s.artist.source, s.artist.id) ? "userCheck" : "userPlus"}
            label={library.isFollowing(s.artist.source, s.artist.id) ? t("artist.following") : t("artist.follow")}
            variant="solid"
            onclick={() => follow(s)} />
        </div>
      </article>
    {/each}
  </div>
{/if}

<style>
  .lead {
    margin: 6px 0 var(--s5);
    color: var(--text-2);
  }
  .center {
    display: grid;
    place-items: center;
    height: 40vh;
  }
  .list {
    display: grid;
    gap: var(--s2);
  }
  article {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--s4);
    padding: var(--s3);
    border-radius: var(--r-lg);
    background: rgba(var(--ink), 0.04);
  }
  .who {
    display: grid;
    justify-items: center;
    gap: 4px;
    min-width: 0;
    padding: 6px;
    border: 0;
    border-radius: var(--r-md);
    background: none;
    text-align: center;
  }
  .who:hover {
    background: var(--hover);
  }
  .name {
    max-width: 100%;
    margin-top: 6px;
    font-size: var(--fs-md);
    font-weight: 650;
  }
  .because {
    max-width: 100%;
    font-size: var(--fs-xs);
    color: var(--text-2);
  }
  .songs {
    min-width: 0;
  }
  .loading {
    display: grid;
    place-items: center;
    height: 150px;
  }
  .none {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--text-3);
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: var(--s2);
  }
</style>
