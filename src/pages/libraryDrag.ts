// Drag sources and drop targets for playlists and folders (library grid, folder pages, sidebar).
import type { Folder, PlaylistSummary } from "../lib/api";
import { dnd, type Payload } from "../lib/stores/dnd.svelte";
import { library } from "../lib/stores/library.svelte";

export function playlistPress(e: PointerEvent, p: PlaylistSummary): void {
  dnd.arm(e, () => ({ kind: "playlist", id: p.id, label: p.name }));
}

export function folderPress(e: PointerEvent, f: Folder): void {
  dnd.arm(e, () => ({ kind: "folder", id: f.id, label: f.name }));
}

/** Songs dropped on a playlist are added to it. */
export function playlistDrop(id: string) {
  return {
    key: `playlist:${id}`,
    accepts: (p: Payload) => p.kind === "tracks" && p.fromPlaylist !== id,
    drop: (p: Payload) => {
      if (p.kind === "tracks") library.addToPlaylist(id, p.tracks);
    },
  };
}

/** Playlists dropped on a folder move into it; a top-level folder can take a folder (one level). */
export function folderDrop(id: string) {
  const folder = () => library.overview.folders.find((f) => f.id === id);
  return {
    key: `folder:${id}`,
    accepts: (p: Payload) => {
      if (p.kind === "playlist") return library.overview.playlists.find((x) => x.id === p.id)?.folderId !== id;
      if (p.kind === "folder") {
        const dragged = library.overview.folders.find((f) => f.id === p.id);
        const hasChildren = library.overview.folders.some((f) => f.parentId === p.id);
        return p.id !== id && !folder()?.parentId && dragged?.parentId !== id && !hasChildren;
      }
      return false;
    },
    drop: (p: Payload) => {
      if (p.kind === "playlist") library.moveToFolder(p.id, id);
      if (p.kind === "folder") library.nestFolder(p.id, id);
    },
  };
}
