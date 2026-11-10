<!-- Playlists, Liked Songs and Local music: one page, three sources of songs. -->
<script lang="ts">
  import { untrack } from "svelte";
  import { api, artistLine, formatLength, type Item, type PlaylistSummary, type Skipped } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import SearchBox from "../lib/components/SearchBox.svelte";
  import Spinner from "../lib/components/Spinner.svelte";
  import TrackList from "../lib/components/TrackList.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { playsFrom } from "../lib/navigate";
  import { openPlaylistMenu } from "../lib/playlistMenu";
  import { dialogs } from "../lib/stores/dialogs.svelte";
  import { history } from "../lib/stores/history.svelte";
  import { fail, library } from "../lib/stores/library.svelte";
  import { menu, type MenuItem } from "../lib/stores/menu.svelte";
  import { offline } from "../lib/stores/offline.svelte";
  import { settings } from "../lib/stores/settings.svelte";
  import { player } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import { fold, matchesQuery } from "../lib/text";
  import EmptyState from "./EmptyState.svelte";
  import ListHeader from "./ListHeader.svelte";

  type Sort = "custom" | "added" | "title" | "artist" | "length" | "plays";
  type SourceFilter = "all" | "youtube" | "soundcloud" | "local";
  type DownloadFilter = "all" | "yes" | "no";

  let { mode, id = "" }: { mode: "playlist" | "liked" | "local" | "offline"; id?: string } = $props();

  let items = $state<Item[]>([]);
  let playlist = $state<PlaylistSummary | null>(null);
  let skipped = $state<Skipped[]>([]);
  let loading = $state(true);
  let failed = $state(false);
  let query = $state("");
  let sort = $state<Sort>(untrack(() => (mode === "playlist" ? "custom" : "added")));
  let artistFilter = $state<string | null>(null);
  let sourceFilter = $state<SourceFilter>("all");
  let downloadFilter = $state<DownloadFilter>("all");
  let seq = 0;

  async function load() {
    const mine = ++seq;
    failed = false;
    try {
      if (mode === "playlist") {
        const d = await api.playlistGet(id);
        if (mine !== seq) return;
        playlist = d.playlist;
        items = d.items;
        skipped = d.skipped;
      } else if (mode === "liked") {
        const list = await api.likedList();
        if (mine === seq) items = list;
      } else if (mode === "offline") {
        const lists = await api.offlineLists();
        // Cached songs show here only if the user wants them in the library, or while offline.
        const withCache = settings.values.showCachedInLibrary || !offline.online;
        const all = withCache ? [...lists.downloaded, ...lists.cached] : lists.downloaded;
        if (mine === seq) items = all.map((track, i) => ({ itemId: track.id, addedAt: all.length - i, track }));
      } else {
        const list = await api.localList();
        // Local files have no "added" date of their own; the list comes newest first.
        if (mine === seq) items = list.map((track, i) => ({ itemId: track.id, addedAt: list.length - i, track }));
      }
    } catch (e) {
      if (mine !== seq) return;
      if (mode === "playlist") {
        // The playlist is gone (deleted elsewhere): go back to the library.
        ui.replace({ name: "library" });
        return;
      }
      failed = true;
      console.error(e);
    } finally {
      if (mine === seq) loading = false;
    }
  }

  $effect(() => {
    void id;
    void library.version;
    if (mode === "offline") {
      void offline.downloaded;
      void offline.cached;
      void offline.online;
      void settings.values.showCachedInLibrary;
    }
    load();
  });

  const folded = $derived(fold(query.trim()));
  const visible = $derived.by(() => {
    let list = offline.playable(items, (i) => i.track).filter((i) => matchesQuery(`${i.track.title} ${artistLine(i.track)} ${i.track.album?.name ?? ""}`, folded));
    if (artistFilter) list = list.filter((i) => i.track.artists.some((a) => a.name === artistFilter));
    if (sourceFilter !== "all") list = list.filter((i) => playsFrom(i.track) === sourceFilter);
    if (downloadFilter !== "all") list = list.filter((i) => offline.isDownloaded(i.track) === (downloadFilter === "yes"));
    const by = (f: (i: Item) => string | number, dir = 1) => [...list].sort((a, b) => (f(a) > f(b) ? dir : f(a) < f(b) ? -dir : 0));
    switch (sort) {
      case "added":
        return mode === "playlist" ? by((i) => i.addedAt, -1) : list;
      case "title":
        return by((i) => fold(i.track.title));
      case "artist":
        return by((i) => fold(artistLine(i.track)));
      case "length":
        return by((i) => i.track.durationMs ?? 0);
      case "plays":
        return by((i) => history.plays(i.track), -1);
      default:
        return list;
    }
  });
  const tracks = $derived(visible.map((i) => i.track));
  const itemIds = $derived(mode === "playlist" ? visible.map((i) => i.itemId) : undefined);
  const totalMs = $derived(items.reduce((sum, i) => sum + (i.track.durationMs ?? 0), 0));
  const unmatched = $derived(items.filter((i) => i.track.via === "none").map((i) => i.track));
  const filtering = $derived(!!folded || !!artistFilter || sourceFilter !== "all" || downloadFilter !== "all");

  const title = $derived(
    mode === "liked" ? t("liked.title") : mode === "local" ? t("local.title") : mode === "offline" ? t("offline.title") : (playlist?.name ?? ""),
  );
  const saved = $derived(mode === "playlist" ? offline.savedPlaylists.has(id) : mode === "liked" ? settings.values.autoDownloadLiked : false);
  const downloadedCount = $derived(items.filter((i) => offline.isDownloaded(i.track)).length);

  function toggleSaved() {
    if (mode === "playlist") offline.setPlaylistSaved(id, !saved);
    else if (mode === "liked") {
      settings.set("autoDownloadLiked", !saved);
      ui.toast(saved ? t("offline.likedOff") : t("offline.likedOn"), "download");
    }
  }
  const cover = $derived(mode === "playlist" ? (playlist?.cover ?? null) : null);
  const chips = $derived.by(() => {
    const list: { text: string; icon?: "music" | "play" | "alert" | "download"; onclick?: () => void }[] = [
      { text: `${t("library.songs", { n: items.length })} · ${formatLength(totalMs, t("time.hourShort"))}`, icon: "music" },
    ];
    if (mode === "playlist" && playlist) list.push({ text: t("playlist.plays", { n: playlist.plays }), icon: "play" });
    if (saved && items.length > 0) {
      const text =
        downloadedCount >= items.length
          ? t("offline.allDownloaded")
          : offline.status.full
            ? t("offline.spaceFull")
            : t("offline.progress", { done: downloadedCount, total: items.length });
      list.push({ text, icon: "download" as const, onclick: offline.status.full ? () => (ui.settingsOpen = true) : undefined });
    }
    if (!offline.online) list.push({ text: t("offline.mode"), icon: "alert" as const, onclick: () => dialogs.open({ kind: "connection" }) });
    const problems = unmatched.length + skipped.length;
    if (problems > 0) {
      list.push({
        text: t("playlist.needsAttention", { n: problems }),
        icon: "alert",
        onclick: () => dialogs.open({ kind: "report", title, playlistId: id || null, unmatched, skipped, truncated: false }),
      });
    }
    return list;
  });

  function playAll(shuffle: boolean) {
    if (tracks.length === 0) return;
    if (player.queue.shuffle !== shuffle) player.toggleShuffle();
    player.playTracks(tracks, shuffle ? Math.floor(Math.random() * tracks.length) : 0, mode === "playlist" ? id : undefined);
  }

  const sourceNames = $derived({ all: t("filter.allSources"), youtube: t("source.youtube"), soundcloud: t("source.soundcloud"), local: t("source.local") });

  function sortMenu(e: MouseEvent) {
    const options: [Sort, string][] = [
      ...(mode === "playlist" ? ([["custom", t("sort.custom")]] as [Sort, string][]) : []),
      ["added", mode === "liked" ? t("sort.liked") : t("sort.added")],
      ["title", t("sort.title")],
      ["artist", t("sort.artist")],
      ["length", t("sort.length")],
      ["plays", t("sort.plays")],
    ];
    menu.show(e, options.map(([value, label]) => ({ label, icon: sort === value ? "check" : "sort", action: () => (sort = value) })));
  }

  function filterMenu(e: MouseEvent) {
    const sources: SourceFilter[] = ["all", "youtube", "soundcloud", "local"];
    const items: MenuItem[] = sources.map((value) => ({
      label: sourceNames[value],
      icon: sourceFilter === value ? "check" : "filter",
      action: () => (sourceFilter = value),
    }));
    const downloads: [DownloadFilter, string][] = [
      ["yes", t("filter.downloaded")],
      ["no", t("filter.notDownloaded")],
    ];
    downloads.forEach(([value, label], k) =>
      items.push({
        label,
        icon: downloadFilter === value ? "check" : "download",
        divider: k === 0,
        action: () => (downloadFilter = downloadFilter === value ? "all" : value),
      }),
    );
    items.push({ label: t("filter.byArtist"), icon: "user", submenu: true, divider: true, action: () => menu.replace(artistItems()) });
    menu.show(e, items);
  }

  function artistItems(): MenuItem[] {
    const counts = new Map<string, number>();
    for (const i of items) for (const a of i.track.artists) counts.set(a.name, (counts.get(a.name) ?? 0) + 1);
    const names = [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, 60);
    return [
      { label: t("filter.allArtists"), icon: artistFilter ? "user" : "check", action: () => (artistFilter = null) },
      ...names.map(([name, n]) => ({ label: `${name} (${n})`, icon: artistFilter === name ? ("check" as const) : ("user" as const), action: () => (artistFilter = name) })),
    ];
  }

  function clearFilters() {
    query = "";
    artistFilter = null;
    sourceFilter = "all";
    downloadFilter = "all";
  }

  async function addFiles() {
    await api.localAddDialog().catch(fail);
  }
</script>

{#if loading && items.length === 0 && !playlist}
  <div class="center"><Spinner size={22} /></div>
{:else if failed}
  <EmptyState icon="alert" title={t("error.loadList")}>
    <Button variant="secondary" icon="refresh" onclick={load}>{t("dialog.retry")}</Button>
  </EmptyState>
{:else}
  <div class="page" class:drop-ready={mode === "local" && ui.filesOver}>
    <ListHeader
      kind={mode === "playlist" ? t("playlist.kind") : mode === "liked" ? t("liked.kind") : mode === "offline" ? t("offline.kind") : t("local.kind")}
      {title}
      {cover}
      icon={mode === "liked" ? "heart" : mode === "local" ? "folder" : mode === "offline" ? "download" : "list"}
      {chips}>
      {#snippet subtitle()}
        {#if mode === "local"}{t("local.sub")}{:else if mode === "offline"}{t("offline.sub")}{/if}
      {/snippet}
      {#snippet actions()}
        <Button variant="primary" icon="play" disabled={tracks.length === 0} onclick={() => playAll(false)}>{t("playlist.playAll")}</Button>
        <IconButton icon="shuffle" label={t("playlist.shuffle")} variant="solid" disabled={tracks.length === 0} onclick={() => playAll(true)} />
        <Button
          variant="secondary"
          icon="queue"
          disabled={tracks.length === 0}
          onclick={() => {
            player.addTracks(tracks);
            ui.toast(t("toast.addedToQueue"), "queue");
          }}>{t("menu.addToQueue")}</Button>
        {#if mode === "local"}
          <Button variant="secondary" icon="plus" onclick={addFiles}>{t("local.addFiles")}</Button>
        {:else if mode === "playlist" || mode === "liked"}
          <Button variant="secondary" icon={saved ? "check" : "download"} onclick={toggleSaved}>{saved ? t("offline.saved") : t("offline.save")}</Button>
        {/if}
      {/snippet}
      {#snippet tools()}
        <SearchBox bind:value={query} placeholder={mode === "playlist" ? t("playlist.search") : t("list.search")} />
        {#if mode === "liked" || mode === "playlist"}
          <IconButton
            icon="filter"
            label={t("filter.title")}
            variant="solid"
            active={artistFilter != null || sourceFilter !== "all" || downloadFilter !== "all"}
            onclick={filterMenu} />
        {/if}
        <IconButton icon="sort" label={t("sort.label")} variant="solid" onclick={sortMenu} />
        {#if mode === "playlist" && playlist}
          <IconButton icon="more" label={t("menu.more")} variant="solid" onclick={(e) => openPlaylistMenu(e, playlist!)} />
        {:else if mode === "liked"}
          <IconButton icon="upload" label={t("liked.import")} variant="solid" onclick={() => dialogs.open({ kind: "importLiked" })} />
        {:else if mode === "offline"}
          <IconButton icon="settings" label={t("settings.page.storage")} variant="solid" onclick={() => (ui.settingsOpen = true)} />
        {:else}
          <IconButton icon="folder" label={t("local.openFolder")} variant="solid" onclick={() => api.localOpenFolder().catch(fail)} />
        {/if}
      {/snippet}
    </ListHeader>

    {#if items.length === 0}
      {#if mode === "playlist"}
        <EmptyState icon="list" title={t("playlist.emptyTitle")} body={t("playlist.emptyBody")} />
      {:else if mode === "liked"}
        <EmptyState icon="heart" title={t("liked.emptyTitle")} body={t("liked.emptyBody")}>
          <Button variant="secondary" icon="download" onclick={() => dialogs.open({ kind: "importLiked" })}>{t("liked.import")}</Button>
        </EmptyState>
      {:else if mode === "offline"}
        <EmptyState icon="download" title={t("offline.emptyTitle")} body={t("offline.emptyBody")} />
      {:else}
        <EmptyState icon="folder" title={t("local.emptyTitle")} body={t("local.emptyBody")}>
          <Button variant="secondary" icon="plus" onclick={addFiles}>{t("local.addFiles")}</Button>
        </EmptyState>
      {/if}
    {:else if visible.length === 0 && !offline.online && !filtering}
      <EmptyState icon="cloud" title={t("offline.noneHereTitle")} body={t("offline.noneHereBody")} />
    {:else if visible.length === 0}
      <div class="none">
        {t("list.noMatches")}
        {#if filtering}<button class="link" onclick={clearFilters}>{t("filter.clear")}</button>{/if}
      </div>
    {:else}
      {#if artistFilter || sourceFilter !== "all" || downloadFilter !== "all"}
        <div class="active-filters">
          {#if sourceFilter !== "all"}<span class="pill">{sourceNames[sourceFilter]}</span>{/if}
          {#if downloadFilter !== "all"}<span class="pill">{downloadFilter === "yes" ? t("filter.downloaded") : t("filter.notDownloaded")}</span>{/if}
          {#if artistFilter}<span class="pill">{artistFilter}</span>{/if}
          <button class="link" onclick={clearFilters}>{t("filter.clear")}</button>
        </div>
      {/if}
      <TrackList {tracks} {itemIds} playlistId={mode === "playlist" ? id : undefined} reorderable={mode === "playlist" && sort === "custom" && !filtering} showAlbum onchanged={load} />
    {/if}
  </div>
{/if}

<style>
  .center {
    display: grid;
    place-items: center;
    height: 50vh;
  }
  .page {
    border-radius: var(--r-lg);
    transition: background var(--t) var(--ease), box-shadow var(--t) var(--ease);
  }
  .drop-ready {
    background: rgba(255, 255, 255, 0.04);
    box-shadow: inset 0 0 0 2px rgba(255, 255, 255, 0.5);
  }
  .none {
    padding: 40px 0;
    text-align: center;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .link {
    margin-left: 8px;
    padding: 0;
    border: 0;
    background: none;
    font: inherit;
    font-weight: 600;
    color: var(--text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .active-filters {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: -8px 0 10px 8px;
    font-size: var(--fs-xs);
  }
  .pill {
    padding: 3px 9px;
    border-radius: var(--r-full);
    background: var(--selected);
    font-weight: 600;
  }
</style>
