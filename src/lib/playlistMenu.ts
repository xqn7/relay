// Menus for playlists and folders (right-click on tiles and sidebar entries, ⋮ on pages).
import { api, type PlaylistSummary } from "./api";
import { t } from "./i18n/index.svelte";
import { dialogs } from "./stores/dialogs.svelte";
import { fail, library } from "./stores/library.svelte";
import { menu, type MenuItem } from "./stores/menu.svelte";
import { player } from "./stores/player.svelte";
import { ui } from "./stores/ui.svelte";

async function playPlaylist(id: string, shuffle: boolean): Promise<void> {
  try {
    const detail = await api.playlistGet(id);
    const tracks = detail.items.map((i) => i.track);
    if (tracks.length === 0) return;
    if (player.queue.shuffle !== shuffle) player.toggleShuffle();
    player.playTracks(tracks, shuffle ? Math.floor(Math.random() * tracks.length) : 0, id);
  } catch (e) {
    fail(e);
  }
}

function folderTargets(p: PlaylistSummary): MenuItem[] {
  const items: MenuItem[] = [];
  if (p.folderId) items.push({ label: t("folder.moveOut"), icon: "library", action: () => library.moveToFolder(p.id, null) });
  for (const f of library.overview.folders.filter((f) => f.id !== p.folderId)) {
    items.push({ label: f.name, icon: "folder", action: () => library.moveToFolder(p.id, f.id) });
  }
  items.push({ label: t("folder.new"), icon: "folderPlus", divider: items.length > 0, action: () => newFolder(null, p.id) });
  return items;
}

export function playlistMenuItems(p: PlaylistSummary): MenuItem[] {
  const items: MenuItem[] = [
    { label: t("menu.play"), icon: "play", disabled: p.count === 0, action: () => playPlaylist(p.id, false) },
    { label: t("playlist.shuffle"), icon: "shuffle", disabled: p.count === 0, action: () => playPlaylist(p.id, true) },
    {
      label: t("playlist.rename"),
      icon: "pencil",
      divider: true,
      action: () =>
        dialogs.open({
          kind: "prompt",
          title: t("playlist.rename"),
          label: t("playlist.name"),
          value: p.name,
          confirm: t("dialog.save"),
          onConfirm: (name) => api.playlistRename(p.id, name).then(() => library.refresh()).catch(fail),
        }),
    },
    { label: t("playlist.mergeWith"), icon: "merge", action: () => dialogs.open({ kind: "merge", playlistId: p.id }) },
    { label: t("playlist.moveToFolder"), icon: "folder", submenu: true, action: () => menu.replace(folderTargets(p)) },
  ];
  if (p.originUrl) {
    items.push({ label: t("playlist.reimport"), icon: "refresh", action: () => dialogs.open({ kind: "import", url: p.originUrl! }) });
  }
  items.push(
    {
      label: t("playlist.exportM3u"),
      icon: "upload",
      action: () =>
        api
          .exportM3u(p.id)
          .then((ok) => ok && ui.toast(t("toast.exported"), "check"))
          .catch(fail),
    },
    { label: t("playlist.delete"), icon: "trash", divider: true, action: () => library.deletePlaylist(p.id) },
  );
  return items;
}

export function openPlaylistMenu(e: MouseEvent, p: PlaylistSummary): void {
  menu.show(e, playlistMenuItems(p));
}

export function folderMenuItems(id: string): MenuItem[] {
  const f = library.overview.folders.find((x) => x.id === id);
  if (!f) return [];
  const items: MenuItem[] = [
    { label: t("folder.open"), icon: "folder", action: () => ui.open({ name: "folder", id }) },
    {
      label: t("folder.rename"),
      icon: "pencil",
      action: () =>
        dialogs.open({
          kind: "prompt",
          title: t("folder.rename"),
          label: t("folder.name"),
          value: f.name,
          confirm: t("dialog.save"),
          onConfirm: (name) => api.folderRename(id, name).then(() => library.refresh()).catch(fail),
        }),
    },
  ];
  if (!f.parentId) {
    items.push({ label: t("folder.newInside"), icon: "folderPlus", action: () => newFolder(id) });
  } else {
    items.push({ label: t("folder.moveOut"), icon: "library", action: () => library.nestFolder(id, null) });
  }
  items.push({ label: t("folder.delete"), icon: "trash", divider: true, action: () => library.deleteFolder(id) });
  return items;
}

export function openFolderMenu(e: MouseEvent, id: string): void {
  menu.show(e, folderMenuItems(id));
}

/** Asks for a name and creates a folder; `moveIn` puts that playlist into it right away. */
export function newFolder(parentId: string | null = null, moveIn?: string): void {
  dialogs.open({
    kind: "prompt",
    title: t("folder.newTitle"),
    label: t("folder.name"),
    value: "",
    confirm: t("playlist.create"),
    onConfirm: async (name) => {
      try {
        const id = await api.folderCreate(name, parentId);
        if (moveIn) await api.playlistSetFolder(moveIn, id);
        await library.refresh();
      } catch (e) {
        fail(e);
      }
    },
  });
}

export function newPlaylist(folderId: string | null = null): void {
  dialogs.open({
    kind: "prompt",
    title: t("playlist.newTitle"),
    label: t("playlist.name"),
    value: "",
    confirm: t("playlist.create"),
    onConfirm: async (name) => {
      const id = await library.createPlaylist(name, [], folderId);
      if (id) ui.open({ name: "playlist", id });
    },
  });
}
