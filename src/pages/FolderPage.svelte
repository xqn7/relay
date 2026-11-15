<script lang="ts">
  import Button from "../lib/components/Button.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import Tile from "../lib/components/Tile.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { newPlaylist, openFolderMenu, openPlaylistMenu } from "../lib/playlistMenu";
  import { dnd, dropTarget, type Payload } from "../lib/stores/dnd.svelte";
  import { library } from "../lib/stores/library.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import EmptyState from "./EmptyState.svelte";
  import { folderDrop, folderPress, playlistDrop, playlistPress } from "./libraryDrag";

  let { id }: { id: string } = $props();
  const folder = $derived(library.overview.folders.find((f) => f.id === id));
  const subfolders = $derived(library.overview.folders.filter((f) => f.parentId === id));
  const playlists = $derived(library.overview.playlists.filter((p) => p.folderId === id));

  $effect(() => {
    // Deleted (or never loaded): there's nothing to show here.
    if (library.version > 0 && !folder) ui.replace({ name: "library" });
  });

  // The open area of a folder page takes playlists dropped onto it.
  const here = $derived({
    key: `folderpage:${id}`,
    accepts: (p: Payload) => p.kind === "playlist" && library.overview.playlists.find((x) => x.id === p.id)?.folderId !== id,
    drop: (p: Payload) => {
      if (p.kind === "playlist") library.moveToFolder(p.id, id);
    },
  });
</script>

{#if folder}
  <div class="head">
    <div>
      <span class="kind">{t("folder.kind")}</span>
      <h1 class="page-title">{folder.name}</h1>
    </div>
    <div class="tools">
      <Button variant="secondary" icon="plus" onclick={() => newPlaylist(id)}>{t("library.newPlaylist")}</Button>
      <IconButton icon="more" label={t("menu.more")} variant="solid" onclick={(e) => openFolderMenu(e, id)} />
    </div>
  </div>
  <div class="grid stagger" class:over={dnd.target === here.key} use:dropTarget={here}>
    {#each subfolders as f (f.id)}
      <Tile
        title={f.name}
        subtitle={t("library.playlists", { n: library.overview.playlists.filter((p) => p.folderId === f.id).length })}
        icon="folder"
        drop={folderDrop(f.id)}
        onclick={() => ui.open({ name: "folder", id: f.id })}
        onmenu={(e) => openFolderMenu(e, f.id)}
        onpress={(e) => folderPress(e, f)} />
    {/each}
    {#each playlists as p (p.id)}
      <Tile
        title={p.name}
        subtitle={t("library.songs", { n: p.count })}
        cover={p.cover}
        icon="list"
        drop={playlistDrop(p.id)}
        onclick={() => ui.open({ name: "playlist", id: p.id })}
        onmenu={(e) => openPlaylistMenu(e, p)}
        onpress={(e) => playlistPress(e, p)} />
    {/each}
  </div>
  {#if subfolders.length === 0 && playlists.length === 0}
    <EmptyState icon="folder" title={t("folder.emptyTitle")} body={t("folder.emptyBody")} />
  {/if}
{/if}

<style>
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--s4);
    margin-bottom: var(--s5);
  }
  .kind {
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-2);
  }
  .tools {
    display: flex;
    gap: var(--s2);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(168px, 1fr));
    gap: 4px;
    margin: 0 -8px;
    min-height: 220px;
    border-radius: var(--r-lg);
    transition: background var(--t) var(--ease);
  }
  .over {
    background: rgba(var(--ink), 0.05);
  }
</style>
