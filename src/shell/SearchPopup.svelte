<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { api, isAppError, SOURCES, type Collection, type SearchKind, type SearchResults, type SearchSource } from "../lib/api";
  import Cover from "../lib/components/Cover.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import SourceBadge from "../lib/components/SourceBadge.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import TrackList from "../lib/components/TrackList.svelte";
  import type { IconName } from "../lib/icons";
  import { t } from "../lib/i18n/index.svelte";
  import { artistMenu, collectionMenu } from "../lib/pins";
  import { offline } from "../lib/stores/offline.svelte";
  import { player, sourceLabel } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";

  const DEBOUNCE_MS = 260;
  type Status = { state: "loading" } | { state: "done"; data: SearchResults } | { state: "error"; offline: boolean };

  let query = $state("");
  let kind = $state<SearchKind>("all");
  let results = $state<Partial<Record<SearchSource, Status>>>({});
  let history = $state<string[]>([]);
  let input = $state<HTMLInputElement>();
  let timer = 0;
  let seq = 0;

  const tabs: { kind: SearchKind; icon: IconName; label: () => string }[] = [
    { kind: "all", icon: "grid", label: () => t("search.all") },
    { kind: "songs", icon: "music", label: () => t("search.songs") },
    { kind: "artists", icon: "user", label: () => t("search.artists") },
    { kind: "albums", icon: "disc", label: () => t("search.albums") },
    { kind: "playlists", icon: "list", label: () => t("search.playlists") },
  ];

  // SoundCloud has no albums or artist catalog worth showing; only ask the sources that can answer.
  const sourcesFor = (k: SearchKind): SearchSource[] => (k === "albums" || k === "artists" ? ["youtube"] : SOURCES);

  $effect(() => {
    input?.focus();
    api.searchHistory().then((h) => (history = h)).catch(() => {});
  });

  $effect(() => {
    const q = query.trim();
    const k = kind;
    clearTimeout(timer);
    const id = ++seq;
    if (!q) {
      results = {};
      return;
    }
    timer = window.setTimeout(() => run(q, k, id), DEBOUNCE_MS);
  });

  function run(q: string, k: SearchKind, id: number) {
    const sources = sourcesFor(k);
    results = Object.fromEntries(sources.map((s) => [s, { state: "loading" }]));
    for (const source of sources) {
      api.search(source, q, k).then(
        (data) => id === seq && (results[source] = { state: "done", data }),
        (e) => id === seq && (results[source] = { state: "error", offline: isAppError(e) && (e.code === "offline" || e.code === "network") }),
      );
    }
  }

  const done = $derived(SOURCES.map((s) => results[s]).filter((r): r is Extract<Status, { state: "done" }> => r?.state === "done").map((r) => r.data));
  // In "All", each source gets its own share so SoundCloud songs aren't pushed out by YouTube's.
  const perSource = $derived(kind === "all" ? { youtube: 8, soundcloud: 5 } : { youtube: 40, soundcloud: 30 });
  const tracks = $derived(done.flatMap((d) => d.tracks.slice(0, perSource[d.source])));
  const artists = $derived(done.flatMap((d) => d.artists).slice(0, kind === "all" ? 3 : 40));
  const albums = $derived(done.flatMap((d) => d.albums).slice(0, kind === "all" ? 4 : 40));
  const playlists = $derived(done.flatMap((d) => d.playlists).slice(0, kind === "all" ? 4 : 40));
  const pending = $derived(Object.entries(results).filter(([, r]) => r?.state === "loading").map(([s]) => s as SearchSource));
  const failed = $derived(Object.entries(results).filter(([, r]) => r?.state === "error").map(([s]) => s as SearchSource));
  const nothing = $derived(pending.length === 0 && tracks.length + artists.length + albums.length + playlists.length === 0);

  function remember() {
    const q = query.trim();
    if (q) api.searchHistoryAdd(q).catch(() => {});
  }

  function close() {
    ui.searchOpen = false;
  }

  async function openCollection(c: Collection) {
    remember();
    if (c.kind === "album") {
      ui.open({ name: "album", source: c.source, id: c.id });
      return;
    }
    try {
      const list = await api.collectionTracks(c.source, c.kind, c.id);
      if (list.length) player.playTracks(list, 0);
      else ui.toast(t("toast.collectionFailed"), "alert");
    } catch {
      ui.toast(t("toast.collectionFailed"), "alert");
    }
  }

  function openArtist(source: SearchSource, id: string, name: string) {
    remember();
    ui.open({ name: "artist", source, id, artistName: name });
  }

  async function clearHistory() {
    await api.searchHistoryClear().catch(() => {});
    history = [];
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="scrim" role="presentation" onpointerdown={close} transition:fade={{ duration: 160 }}></div>
<div class="popup" transition:fly={{ y: -8, duration: 180 }}>
  <div class="box glass">
    <Icon name="search" size={18} />
    <input bind:this={input} bind:value={query} placeholder={t("search.placeholder")} spellcheck="false" onkeydown={(e) => e.key === "Enter" && remember()} />
    {#if query}
      <button class="clear" aria-label={t("search.clearInput")} onclick={() => ((query = ""), input?.focus())}><Icon name="close" size={14} /></button>
    {/if}
  </div>

  <div class="results glass">
    {#if !offline.online}
      <button class="status offline" onclick={() => ui.open({ name: "offline" })}><Icon name="cloud" size={16} />{t("offline.search")}</button>
    {/if}
    {#if !query.trim()}
      {#if history.length}
        <div class="head">
          <h4>{t("search.recent")}</h4>
          <button class="link" onclick={clearHistory}>{t("search.clear")}</button>
        </div>
        {#each history as h (h)}
          <button class="history" onclick={() => (query = h)}><Icon name="history" size={16} /><span class="truncate">{h}</span></button>
        {/each}
      {:else}
        <p class="hint">{t("search.placeholder")}</p>
      {/if}
    {:else}
      <div class="tabs" role="tablist">
        {#each tabs as tab (tab.kind)}
          <button role="tab" aria-selected={kind === tab.kind} class:on={kind === tab.kind} onclick={() => (kind = tab.kind)}>
            <Icon name={tab.icon} size={14} />{tab.label()}
          </button>
        {/each}
      </div>
      <!-- Any click on a result means the query was useful, so it goes into the history. -->
      <div class="scroll" role="presentation" onclick={remember}>
        {#if artists.length}
          {#each artists as a (a.source + a.id)}
            <button class="entity" onclick={() => openArtist(a.source, a.id, a.name)} oncontextmenu={(e) => artistMenu(e, a)}>
              <Cover src={a.avatar} size={44} round fallback="user" />
              <span class="name truncate">{a.name}</span>
            </button>
          {/each}
        {/if}
        {#if tracks.length}
          {#if kind === "all"}<h4>{t("search.songs")}</h4>{/if}
          <TrackList {tracks} compact />
        {/if}
        {#each [{ list: albums, label: t("search.albums"), icon: "disc" as IconName }, { list: playlists, label: t("search.playlists"), icon: "list" as IconName }] as group (group.label)}
          {#if group.list.length}
            {#if kind === "all"}<h4>{group.label}</h4>{/if}
            {#each group.list as c (c.source + c.id)}
              <button class="entity" onclick={() => openCollection(c)} oncontextmenu={(e) => collectionMenu(e, c)}>
                <Cover src={c.cover} size={44} fallback={group.icon} />
                <span class="col">
                  <span class="name truncate">{c.title}</span>
                  <span class="sub"><SourceBadge source={c.source} /><span class="truncate">{c.subtitle}</span></span>
                </span>
                <Icon name={c.kind === "album" ? "forward" : "play"} size={14} />
              </button>
            {/each}
          {/if}
        {/each}
        {#each pending as s (s)}
          <div class="status"><Spinner size={16} />{t("search.loading", { source: sourceLabel(s) })}</div>
        {/each}
        {#each failed as s (s)}
          <div class="status"><Icon name="alert" size={16} />{(results[s] as { offline: boolean }).offline ? t("error.offline") : t("search.failed", { source: sourceLabel(s) })}</div>
        {/each}
        {#if nothing && failed.length === 0}
          <p class="hint">{t("search.noResults", { q: query.trim() })}</p>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: rgba(var(--scrim), 0.35);
    backdrop-filter: blur(10px);
  }
  .popup {
    position: fixed;
    top: 54px;
    left: 50%;
    width: min(680px, calc(100vw - 160px));
    transform: translateX(-50%);
    z-index: 61;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .box {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 52px;
    padding: 0 16px;
    border-radius: var(--r-lg);
    background: rgba(var(--panel), 0.82);
    color: var(--text-2);
  }
  .box input {
    flex: 1;
    border: 0;
    outline: none;
    background: transparent;
    font-size: var(--fs-lg);
    color: var(--text);
  }
  .box input::placeholder {
    color: var(--text-3);
  }
  .clear {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border: 0;
    border-radius: 50%;
    background: rgba(var(--ink), 0.14);
    color: var(--text);
  }
  .results {
    max-height: min(560px, calc(100vh - 220px));
    display: flex;
    flex-direction: column;
    padding: 10px;
    border-radius: var(--r-lg);
    background: rgba(var(--panel), 0.82);
  }
  .scroll {
    overflow-y: auto;
    margin: 0 -4px;
    padding: 0 4px;
  }
  .tabs {
    display: flex;
    gap: 4px;
    margin-bottom: 8px;
  }
  .tabs button {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--text-2);
  }
  .tabs button:hover {
    color: var(--text);
  }
  .tabs .on {
    background: var(--glass-raised);
    color: var(--text);
  }
  h4 {
    margin: 12px 8px 4px;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .head h4 {
    margin-top: 4px;
  }
  .link {
    border: 0;
    background: none;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .link:hover {
    color: var(--text);
  }
  .history,
  .entity {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 8px;
    border: 0;
    border-radius: var(--r-md);
    background: transparent;
    text-align: left;
    color: var(--text);
  }
  .entity {
    padding: 6px;
  }
  .entity :global(svg:last-child) {
    margin-left: auto;
    color: var(--text-3);
    opacity: 0;
  }
  .entity:hover :global(svg:last-child) {
    opacity: 1;
  }
  .history {
    color: var(--text-2);
  }
  .history:hover,
  .entity:hover {
    background: var(--hover);
    color: var(--text);
  }
  .col {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .sub {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 2px;
    font-size: var(--fs-sm);
    color: var(--text-2);
    min-width: 0;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 8px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .status.offline {
    width: 100%;
    border: 0;
    border-radius: var(--r-md);
    background: var(--glass-strong);
    font: inherit;
    font-size: var(--fs-sm);
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .hint {
    margin: 10px 8px;
    font-size: var(--fs-sm);
    color: var(--text-3);
  }
</style>
