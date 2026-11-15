<script lang="ts">
  import { fade, fly } from "svelte/transition";
  import { api, isAppError, SOURCES, type Collection, type LyricHit, type RemoteList, type SearchKind, type SearchResults, type SearchSource } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import { dialogs } from "../lib/stores/dialogs.svelte";
  import { errorText } from "./dialogs/importText";
  import Cover from "../lib/components/Cover.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import SourceBadge from "../lib/components/SourceBadge.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import TrackList from "../lib/components/TrackList.svelte";
  import type { IconName } from "../lib/icons";
  import { t } from "../lib/i18n/index.svelte";
  import { artistMenu, collectionMenu } from "../lib/pins";
  import { offline } from "../lib/stores/offline.svelte";
  import { warmNow } from "../lib/player/warm";
  import { cachedSearch, rememberSearch } from "../lib/searchCache";
  import { player, sourceLabel } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";

  const DEBOUNCE_MS = 260;
  type Status = { state: "loading" } | { state: "done"; data: SearchResults } | { state: "error"; offline: boolean };

  type LinkStatus = { state: "loading" } | { state: "done"; list: RemoteList; url: string } | { state: "error"; text: string };

  let query = $state("");
  /** A pasted link is opened, not searched for. */
  let link = $state<LinkStatus | null>(null);
  /** "lyrics" finds songs by a line of their words instead of by name. */
  let kind = $state<SearchKind | "lyrics">("all");
  /** Show only one service's results; the other stays loaded for switching back. */
  let only = $state<SearchSource | null>(null);
  let lyricHits = $state<{ state: "loading" } | { state: "done"; hits: LyricHit[] } | { state: "error"; offline: boolean } | null>(null);
  let opening = $state<LyricHit | null>(null);
  let results = $state<Partial<Record<SearchSource, Status>>>({});
  let history = $state<string[]>([]);
  let input = $state<HTMLInputElement>();
  let timer = 0;
  let seq = 0;

  const tabs: { kind: SearchKind | "lyrics"; icon: IconName; label: () => string }[] = [
    { kind: "all", icon: "grid", label: () => t("search.all") },
    { kind: "songs", icon: "music", label: () => t("search.songs") },
    { kind: "artists", icon: "user", label: () => t("search.artists") },
    { kind: "albums", icon: "disc", label: () => t("search.albums") },
    { kind: "playlists", icon: "list", label: () => t("search.playlists") },
    { kind: "lyrics", icon: "micVocal", label: () => t("search.lyrics") },
  ];

  function toggleOnly(source: SearchSource) {
    only = only === source ? null : source;
  }

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
      link = null;
      return;
    }
    if (isLink(q)) {
      results = {};
      // At once, so the previous link's card or the search tabs don't linger under a new link.
      link = { state: "loading" };
      timer = window.setTimeout(() => openLink(q, id), DEBOUNCE_MS);
      return;
    }
    link = null;
    if (k === "lyrics") {
      results = {};
      lyricHits = { state: "loading" };
      timer = window.setTimeout(() => findByLyrics(q, id), DEBOUNCE_MS);
      return;
    }
    lyricHits = null;
    // Searched a moment ago: no need to wait for the typing to settle.
    const known = sourcesFor(k).every((s) => cachedSearch(s, k, q));
    timer = window.setTimeout(() => run(q, k, id), known ? 0 : DEBOUNCE_MS);
  });

  async function findByLyrics(q: string, id: number) {
    try {
      const hits = await api.lyricsSearch(q);
      if (id === seq) lyricHits = { state: "done", hits };
    } catch (e) {
      if (id === seq) lyricHits = { state: "error", offline: isAppError(e) && (e.code === "offline" || e.code === "network") };
    }
  }

  async function playHit(hit: LyricHit) {
    opening = hit;
    try {
      const track = await api.lyricsHitTrack(hit);
      remember();
      player.playTracks([track], 0);
      close();
    } catch {
      ui.toast(t("search.lyricsNotFound", { title: hit.title }), "alert");
    } finally {
      opening = null;
    }
  }

  // Links from the services Relay can read (the same ones the Import box takes).
  const LINK = /^(https?:\/\/|spotify:)|^(www\.)?(open\.spotify\.com|spotify\.link|music\.apple\.com|(www\.)?deezer\.com|deezer\.page\.link|(music\.)?youtube\.com|youtu\.be|(m\.)?soundcloud\.com|on\.soundcloud\.com)\//i;
  const isLink = (q: string) => LINK.test(q) && !/\s/.test(q);

  async function openLink(url: string, id: number) {
    try {
      const list = await api.importPreview(url);
      if (id !== seq) return;
      link = { state: "done", list, url };
      if (list.tracks[0]) warmNow(list.tracks[0]);
    } catch (e) {
      if (id === seq) link = { state: "error", text: errorText(e) };
    }
  }

  function playLink(list: RemoteList) {
    if (list.tracks.length) player.playTracks(list.tracks, 0);
    close();
  }

  function importLink(url: string) {
    close();
    dialogs.open({ kind: "import", url });
  }

  function run(q: string, k: SearchKind, id: number) {
    const sources = sourcesFor(k);
    results = Object.fromEntries(
      sources.map((s) => {
        const data = cachedSearch(s, k, q);
        return [s, data ? { state: "done", data } : { state: "loading" }];
      }),
    );
    for (const source of sources) {
      if (results[source]?.state === "done") continue;
      api.search(source, q, k).then(
        (data) => {
          rememberSearch(source, k, q, data);
          if (id !== seq) return;
          results[source] = { state: "done", data };
          // The top song is the most likely pick; look it up before the click.
          if (source === "youtube" && data.tracks[0]) warmNow(data.tracks[0]);
        },
        (e) => id === seq && (results[source] = { state: "error", offline: isAppError(e) && (e.code === "offline" || e.code === "network") }),
      );
    }
  }

  const shown = $derived(only ? [only] : SOURCES);
  const done = $derived(shown.map((s) => results[s]).filter((r): r is Extract<Status, { state: "done" }> => r?.state === "done").map((r) => r.data));
  // In "All", each source gets its own share so SoundCloud songs aren't pushed out by YouTube's.
  const perSource = $derived(kind === "all" ? { youtube: 8, soundcloud: 5 } : { youtube: 40, soundcloud: 30 });
  const tracks = $derived(done.flatMap((d) => d.tracks.slice(0, perSource[d.source])));
  const artists = $derived(done.flatMap((d) => d.artists).slice(0, kind === "all" ? 3 : 40));
  const albums = $derived(done.flatMap((d) => d.albums).slice(0, kind === "all" ? 4 : 40));
  const playlists = $derived(done.flatMap((d) => d.playlists).slice(0, kind === "all" ? 4 : 40));
  const pending = $derived(shown.filter((s) => results[s]?.state === "loading"));
  const failed = $derived(shown.filter((s) => results[s]?.state === "error"));
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
      {#if !link && kind !== "lyrics"}
        <div class="only" role="group" aria-label={t("search.onlyFrom")}>
          {#each SOURCES as s (s)}
            <button class:on={only === s} aria-pressed={only === s} title={t("search.onlySource", { source: sourceLabel(s) })} onclick={() => toggleOnly(s)}>
              <Icon name={s === "youtube" ? "youtube" : "cloud"} size={15} />
            </button>
          {/each}
        </div>
      {/if}
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
    {:else if link}
      <div class="scroll">
        {#if link.state === "loading"}
          <div class="status"><Spinner size={16} />{t("import.loadingPreview")}</div>
        {:else if link.state === "error"}
          <div class="status"><Icon name="alert" size={16} />{link.text}</div>
        {:else if link.list.kind === "track"}
          <h4>{t("search.fromLink")}</h4>
          <TrackList tracks={link.list.tracks} compact warmFirst={false} />
        {:else}
          {@const list = link.list}
          {@const url = link.url}
          <h4>{t("search.fromLink")}</h4>
          <div class="link-card">
            <Cover src={list.cover} size={64} fallback={list.kind === "album" ? "disc" : "list"} />
            <span class="col">
              <span class="name truncate">{list.title}</span>
              <span class="sub truncate">{list.kind === "album" ? t("import.kindAlbum") : t("import.kindPlaylist")}{list.owner ? ` · ${list.owner}` : ""} · {t("import.songCount", { n: list.tracks.length })}</span>
            </span>
            <span class="link-actions">
              <Button variant="primary" small icon="play" disabled={!list.tracks.length} onclick={() => playLink(list)}>{t("menu.play")}</Button>
              <Button variant="secondary" small icon="download" onclick={() => importLink(url)}>{t("import.start")}</Button>
            </span>
          </div>
        {/if}
      </div>
    {:else}
      <div class="tabs" role="tablist">
        {#each tabs as tab (tab.kind)}
          <button role="tab" aria-selected={kind === tab.kind} class:on={kind === tab.kind} onclick={() => (kind = tab.kind)}>
            <Icon name={tab.icon} size={14} />{tab.label()}
          </button>
        {/each}
      </div>
      {#if kind === "lyrics"}
        <div class="scroll">
          {#if lyricHits?.state === "loading"}
            <div class="status"><Spinner size={16} />{t("search.lyricsLoading")}</div>
          {:else if lyricHits?.state === "error"}
            <div class="status"><Icon name="alert" size={16} />{lyricHits.offline ? t("error.offline") : t("search.lyricsFailed")}</div>
          {:else if lyricHits?.state === "done" && lyricHits.hits.length === 0}
            <p class="hint">{t("search.lyricsNone")}</p>
          {:else if lyricHits?.state === "done"}
            {#each lyricHits.hits as hit (hit.artist + hit.title)}
              <button class="entity lyric" disabled={!!opening} onclick={() => playHit(hit)}>
                <span class="mark">{#if opening === hit}<Spinner size={16} />{:else}<Icon name="play" size={14} />{/if}</span>
                <span class="col">
                  <span class="name truncate">{hit.title} <span class="by">· {hit.artist}</span></span>
                  <span class="sub line truncate">“{hit.line}”</span>
                </span>
              </button>
            {/each}
          {/if}
        </div>
      {:else}
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
          <TrackList {tracks} compact warmFirst={false} />
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
  .only {
    display: flex;
    gap: 2px;
  }
  .only button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
    transition:
      background var(--t) var(--ease),
      color var(--t) var(--ease);
  }
  .only button:hover {
    color: var(--text);
  }
  .only button.on {
    background: rgba(var(--ink), 0.14);
    color: var(--text);
  }
  .lyric .mark {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: var(--r-sm);
    background: rgba(var(--ink), 0.08);
    color: var(--text-2);
  }
  .lyric .by {
    font-weight: 400;
    color: var(--text-3);
  }
  .lyric .line {
    font-style: italic;
  }
  .link-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 8px;
  }
  .link-card .col {
    flex: 1;
    min-width: 0;
  }
  .link-actions {
    display: flex;
    gap: 6px;
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
