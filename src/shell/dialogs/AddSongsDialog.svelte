<!-- "Add songs" on a playlist: search YouTube and SoundCloud and add songs right into it, one
     click each, without leaving the playlist. -->
<script lang="ts">
  import { onDestroy } from "svelte";
  import { api, artistLine, SOURCES, type Track } from "../../lib/api";
  import Cover from "../../lib/components/Cover.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import SearchBox from "../../lib/components/SearchBox.svelte";
  import SourceBadge from "../../lib/components/SourceBadge.svelte";
  import Spinner from "../../lib/components/Spinner.svelte";
  import { explain } from "../../lib/errors";
  import { t } from "../../lib/i18n/index.svelte";
  import { cachedSearch, rememberSearch } from "../../lib/searchCache";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { library } from "../../lib/stores/library.svelte";

  let { playlistId, name }: { playlistId: string; name: string } = $props();

  const DEBOUNCE_MS = 260;
  /** Songs per service, so SoundCloud's aren't pushed out by YouTube's. */
  const PER_SOURCE = 12;

  let query = $state("");
  let results = $state<Track[]>([]);
  let loading = $state(false);
  let error = $state("");
  let added = $state<Set<string>>(new Set());
  let adding = $state<string | null>(null);
  let timer = 0;
  let seq = 0;

  $effect(() => {
    const q = query.trim();
    clearTimeout(timer);
    const id = ++seq;
    if (!q) {
      results = [];
      loading = false;
      return;
    }
    loading = true;
    timer = window.setTimeout(() => search(q, id), DEBOUNCE_MS);
  });

  async function search(q: string, id: number) {
    error = "";
    const found = await Promise.allSettled(
      SOURCES.map(async (source) => {
        const cached = cachedSearch(source, "songs", q);
        if (cached) return cached;
        const data = await api.search(source, q, "songs");
        rememberSearch(source, "songs", q, data);
        return data;
      }),
    );
    if (id !== seq) return;
    results = found.flatMap((r) => (r.status === "fulfilled" ? r.value.tracks.slice(0, PER_SOURCE) : []));
    const failure = found.find((r): r is PromiseRejectedResult => r.status === "rejected");
    if (!results.length && failure) error = explain(failure.reason).name;
    loading = false;
  }

  async function add(track: Track) {
    adding = track.id;
    try {
      await api.playlistAdd(playlistId, [track]);
      added = new Set(added).add(track.id);
    } catch (e) {
      error = explain(e).name;
    } finally {
      adding = null;
    }
  }

  // The playlist page shows the new songs once this closes.
  onDestroy(() => {
    if (added.size) library.refresh().catch(() => {});
  });
</script>

<Dialog title={t("addSongs.title", { name })} sub={added.size ? t("addSongs.added", { n: added.size }) : t("addSongs.sub")} width={520} onclose={() => dialogs.close()}>
  <div class="box"><SearchBox bind:value={query} placeholder={t("addSongs.placeholder")} width={472} /></div>
  <div class="list">
    {#if loading}
      <div class="state"><Spinner size={18} /></div>
    {:else if error}
      <p class="state">{error}</p>
    {:else if query.trim() && !results.length}
      <p class="state">{t("search.noResults", { q: query.trim() })}</p>
    {:else}
      {#each results as track (track.id)}
        {@const done = added.has(track.id)}
        <div class="row">
          <Cover src={track.cover} size={40} />
          <div class="text">
            <div class="title truncate">{track.title}</div>
            <div class="artist"><SourceBadge source={track.source} /><span class="truncate">{artistLine(track)}</span></div>
          </div>
          <button class="add" class:done disabled={done || adding === track.id} aria-label={done ? t("addSongs.inPlaylist") : t("addSongs.add")} title={done ? t("addSongs.inPlaylist") : t("addSongs.add")} onclick={() => add(track)}>
            {#if adding === track.id}<Spinner size={14} />{:else}<Icon name={done ? "check" : "plus"} size={16} />{/if}
          </button>
        </div>
      {/each}
    {/if}
  </div>
</Dialog>

<style>
  .box {
    margin-bottom: 10px;
  }
  .list {
    height: min(420px, 55vh);
    overflow-y: auto;
    margin: 0 -8px;
    padding: 0 8px;
  }
  .state {
    display: grid;
    place-items: center;
    margin: 40px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 8px;
    border-radius: var(--r-md);
  }
  .row:hover {
    background: rgba(var(--ink), 0.06);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-weight: 600;
    font-size: var(--fs-sm);
  }
  .artist {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: var(--fs-xs);
    color: var(--text-2);
  }
  .add {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    flex: none;
    border: 0;
    border-radius: 50%;
    background: rgba(var(--ink), 0.1);
    color: var(--text);
    cursor: pointer;
    transition: background var(--t) var(--ease);
  }
  .add:hover:not(:disabled) {
    background: rgba(var(--ink), 0.2);
  }
  .add.done {
    background: var(--solid);
    color: var(--on-primary);
    cursor: default;
  }
</style>
