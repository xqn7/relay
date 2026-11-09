<script lang="ts">
  import { untrack } from "svelte";
  import { api, type PlaylistSummary } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import SearchBox from "../lib/components/SearchBox.svelte";
  import Segmented from "../lib/components/Segmented.svelte";
  import Tile from "../lib/components/Tile.svelte";
  import { offline } from "../lib/stores/offline.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { newFolder, newPlaylist, openFolderMenu, openPlaylistMenu } from "../lib/playlistMenu";
  import { dialogs } from "../lib/stores/dialogs.svelte";
  import { dnd, dropTarget, type Payload } from "../lib/stores/dnd.svelte";
  import { library } from "../lib/stores/library.svelte";
  import { menu } from "../lib/stores/menu.svelte";
  import { ui, type LibraryTab } from "../lib/stores/ui.svelte";
  import { fold, matchesQuery } from "../lib/text";
  import EmptyState from "./EmptyState.svelte";
  import FollowedArtists from "./FollowedArtists.svelte";
  import { folderDrop, playlistDrop, playlistPress, folderPress } from "./libraryDrag";

  let { tab = "all" }: { tab?: LibraryTab } = $props();
  let current = $state<LibraryTab>(untrack(() => tab));
  let query = $state("");
  let localCount = $state<number | null>(null);
  let trashCount = $state(0);

  $effect(() => {
    void library.version;
    api.localList().then((l) => (localCount = l.length)).catch(() => {});
    api.trashList().then((l) => (trashCount = l.length)).catch(() => {});
  });

  const q = $derived(fold(query.trim()));
  const folders = $derived(library.overview.folders.filter((f) => !f.parentId && matchesQuery(f.name, q)));
  const playlists = $derived(library.overview.playlists.filter((p) => (q ? matchesQuery(p.name, q) : !p.folderId)));
  const show = (name: string) => current === "all" && matchesQuery(name, q);

  function folderCovers(id: string): (string | null)[] {
    const inside = new Set([id, ...library.overview.folders.filter((f) => f.parentId === id).map((f) => f.id)]);
    return library.overview.playlists.filter((p) => p.folderId && inside.has(p.folderId)).map((p) => p.cover).filter(Boolean).slice(0, 4);
  }

  function folderSub(id: string): string {
    const n = library.overview.playlists.filter((p) => p.folderId === id).length;
    return t("library.playlists", { n });
  }

  function playlistSub(p: PlaylistSummary): string {
    return t("library.songs", { n: p.count });
  }

  function createMenu(e: MouseEvent) {
    menu.show(e, [
      { label: t("library.newPlaylist"), icon: "plus", action: () => newPlaylist() },
      { label: t("library.newFolder"), icon: "folderPlus", action: () => newFolder() },
    ]);
  }

  // Dropping a playlist or folder on the open grid takes it out of its folder.
  const rootDrop = {
    key: "root",
    accepts: (p: Payload) =>
      (p.kind === "playlist" && !!library.overview.playlists.find((x) => x.id === p.id)?.folderId) ||
      (p.kind === "folder" && !!library.overview.folders.find((x) => x.id === p.id)?.parentId),
    drop: (p: Payload) => {
      if (p.kind === "playlist") library.moveToFolder(p.id, null);
      if (p.kind === "folder") library.nestFolder(p.id, null);
    },
  };
</script>

<div class="head">
  <h1 class="page-title">{t("library.title")}</h1>
  <div class="tools">
    <SearchBox bind:value={query} placeholder={t("library.search")} />
    <Button variant="secondary" icon="download" onclick={() => dialogs.open({ kind: "import" })}>{t("library.import")}</Button>
    <Button variant="primary" icon="plus" onclick={createMenu}>{t("library.create")}</Button>
  </div>
</div>
<div class="tabs">
  <Segmented
    label={t("library.title")}
    value={current}
    options={[
      { value: "all", label: t("library.all") },
      { value: "playlists", label: t("library.playlistsTab") },
      { value: "artists", label: t("library.artistsTab") },
    ]}
    onchange={(v) => (current = v)} />
</div>

{#if current === "artists"}
  <FollowedArtists />
{:else}
  <div class="grid" class:root-over={dnd.target === "root"} use:dropTarget={rootDrop}>
    {#if show(t("liked.title"))}
      <Tile title={t("liked.title")} subtitle={t("library.songs", { n: library.overview.likedCount })} icon="heart" onclick={() => ui.open({ name: "liked" })} />
    {/if}
    {#if show(t("local.title"))}
      <Tile
        title={t("local.title")}
        subtitle={localCount == null ? "" : t("library.songs", { n: localCount })}
        icon="folder"
        onclick={() => ui.open({ name: "local" })} />
    {/if}
    {#if show(t("offline.title")) && (offline.downloaded.size > 0 || offline.cached.size > 0 || !offline.online)}
      <Tile title={t("offline.title")} subtitle={t("offline.tileSub")} icon="download" onclick={() => ui.open({ name: "offline" })} />
    {/if}
    {#each folders as f (f.id)}
      <Tile
        title={f.name}
        subtitle={folderSub(f.id)}
        covers={folderCovers(f.id)}
        icon="folder"
        drop={folderDrop(f.id)}
        onclick={() => ui.open({ name: "folder", id: f.id })}
        onmenu={(e) => openFolderMenu(e, f.id)}
        onpress={(e) => folderPress(e, f)} />
    {/each}
    {#each playlists as p (p.id)}
      <Tile
        title={p.name}
        subtitle={playlistSub(p)}
        cover={p.cover}
        icon="list"
        drop={playlistDrop(p.id)}
        onclick={() => ui.open({ name: "playlist", id: p.id })}
        onmenu={(e) => openPlaylistMenu(e, p)}
        onpress={(e) => playlistPress(e, p)} />
    {/each}
    {#if show(t("trash.title")) && trashCount > 0}
      <Tile title={t("trash.title")} subtitle={t("trash.items", { n: trashCount })} icon="trash" onclick={() => ui.open({ name: "trash" })} />
    {/if}
  </div>
  {#if current === "playlists" && library.overview.playlists.length === 0 && library.overview.folders.length === 0}
    <EmptyState icon="list" title={t("library.emptyTitle")} body={t("library.emptyBody")}>
      <Button variant="primary" icon="download" onclick={() => dialogs.open({ kind: "import" })}>{t("library.import")}</Button>
    </EmptyState>
  {/if}
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s4);
    flex-wrap: wrap;
  }
  .tools {
    display: flex;
    gap: var(--s2);
  }
  .tabs {
    margin: var(--s4) 0 var(--s4);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(168px, 1fr));
    gap: 4px;
    margin: 0 -8px;
    min-height: 200px;
    border-radius: var(--r-lg);
    transition: background var(--t) var(--ease);
  }
  .root-over {
    background: rgba(255, 255, 255, 0.04);
  }
</style>
