// The song menu (⋮ button and right-click on any song). With several songs selected, the
// actions that make sense for many songs apply to all of them.
import { api, IMPORT_ONLY, type Track } from "./api";
import { t } from "./i18n/index.svelte";
import { hasAlbumPage, openAlbum, openArtist } from "./navigate";
import { dialogs } from "./stores/dialogs.svelte";
import { fail, library } from "./stores/library.svelte";
import { menu, type MenuItem } from "./stores/menu.svelte";
import { player } from "./stores/player.svelte";
import { ui } from "./stores/ui.svelte";

export interface MenuContext {
  /** The list the song is in (played from here with "Play"). */
  tracks: Track[];
  index: number;
  /** Selected songs, when the right-clicked song is part of a multi-selection. */
  selection?: Track[];
  /** Set inside a playlist: enables "Remove from this playlist". */
  playlist?: { id: string; itemIds: string[]; onRemoved: () => void };
}

function playlistItems(tracks: Track[]): MenuItem[] {
  const items: MenuItem[] = [
    {
      label: t("menu.newPlaylist"),
      icon: "plus",
      action: () =>
        dialogs.open({
          kind: "prompt",
          title: t("playlist.newTitle"),
          label: t("playlist.name"),
          value: tracks.length === 1 ? tracks[0].title : "",
          confirm: t("playlist.create"),
          onConfirm: async (name) => {
            const id = await library.createPlaylist(name, tracks);
            if (id) ui.toast(t("toast.addedToPlaylist", { name, n: tracks.length }), "check");
          },
        }),
    },
  ];
  library.overview.playlists.forEach((p, i) =>
    items.push({ label: p.name, icon: "list", divider: i === 0, action: () => library.addToPlaylist(p.id, tracks) }),
  );
  return items;
}

function artistItems(track: Track): MenuItem[] {
  return track.artists.map((a) => ({ label: a.name, icon: "user", action: () => openArtist(a, track.source) }));
}

export function trackMenuItems(ctx: MenuContext): MenuItem[] {
  const track = ctx.tracks[ctx.index];
  const many = ctx.selection && ctx.selection.length > 1 ? ctx.selection : null;
  const targets = many ?? [track];
  const items: MenuItem[] = [];

  if (!many) items.push({ label: t("menu.play"), icon: "play", action: () => player.playTracks(ctx.tracks, ctx.index) });
  items.push(
    {
      label: t("menu.playNext"),
      icon: "playNext",
      action: () => {
        player.playNextTracks(targets);
        ui.toast(t("toast.playingNext"), "playNext");
      },
    },
    {
      label: t("menu.addToQueue"),
      icon: "queue",
      action: () => {
        player.addTracks(targets);
        ui.toast(t("toast.addedToQueue"), "queue");
      },
    },
    { label: t("menu.addToPlaylist"), icon: "listPlus", submenu: true, action: () => menu.replace(playlistItems(targets)) },
  );

  if (ctx.playlist) {
    const pl = ctx.playlist;
    items.push({ label: t("menu.removeFromPlaylist"), icon: "close", action: () => removeFromPlaylist(pl.id, pl.itemIds, pl.onRemoved) });
  }

  if (many) {
    items.push({
      label: t("menu.likeAll"),
      icon: "heart",
      divider: true,
      action: async () => {
        for (const s of many.filter((s) => !library.isLiked(s))) await library.toggleLike(s);
      },
    });
    return items;
  }

  const liked = library.isLiked(track);
  items.push({ label: liked ? t("like.remove") : t("like.add"), icon: "heart", divider: true, action: () => library.toggleLike(track) });
  if (track.artists.length === 1) {
    const a = track.artists[0];
    items.push({ label: t("menu.goToArtist"), icon: "user", action: () => openArtist(a, track.source) });
  } else if (track.artists.length > 1) {
    items.push({ label: t("menu.goToArtist"), icon: "user", submenu: true, action: () => menu.replace(artistItems(track)) });
  }
  if (hasAlbumPage(track)) items.push({ label: t("menu.goToAlbum"), icon: "disc", action: () => openAlbum(track) });
  items.push({ label: t("menu.startRadio"), icon: "radio", action: () => player.startRadio(track) });
  items.push({
    label: t("menu.copyLink"),
    icon: "link",
    disabled: !track.link,
    action: () => {
      if (track.link) navigator.clipboard.writeText(track.link).then(() => ui.toast(t("toast.linkCopied"), "check"));
    },
  });

  if (IMPORT_ONLY.has(track.source)) {
    items.push({ label: t("menu.fixMatch"), icon: "replace", divider: true, action: () => dialogs.open({ kind: "fixMatch", track }) });
  }
  if (track.source === "local") {
    items.push(
      { label: t("menu.editInfo"), icon: "pencil", divider: true, action: () => dialogs.open({ kind: "editLocal", track }) },
      { label: t("menu.removeLocal"), icon: "trash", action: () => removeLocal(track) },
    );
  }

  const songHidden = library.hidden.some((h) => h.kind === "track" && h.value === track.id);
  items.push({
    label: songHidden ? t("menu.allowSong") : t("menu.hideSong"),
    icon: "eyeOff",
    divider: !IMPORT_ONLY.has(track.source) && track.source !== "local",
    action: () => (songHidden ? library.setHidden("track", track.id, "", false) : library.hideTrack(track)),
  });
  const artist = track.artists[0]?.name;
  if (artist) {
    items.push({ label: t("menu.hideArtist", { name: artist }), icon: "eyeOff", action: () => library.hideArtist(artist) });
  }
  return items;
}

export function openTrackMenu(e: MouseEvent, ctx: MenuContext): void {
  menu.show(e, trackMenuItems(ctx));
}

async function removeFromPlaylist(id: string, itemIds: string[], onRemoved: () => void): Promise<void> {
  try {
    const removed = await api.playlistRemove(id, itemIds);
    onRemoved();
    library.refresh().catch(() => {});
    ui.toast(t("toast.removedFromPlaylist", { n: removed.length }), "check", {
      label: t("toast.undo"),
      run: () =>
        api
          .playlistRestoreItems(id, removed)
          .then(() => {
            onRemoved();
            return library.refresh();
          })
          .catch(fail),
    });
  } catch (e) {
    fail(e);
  }
}

async function removeLocal(track: Track): Promise<void> {
  try {
    await api.localRemove(track.sourceId);
    ui.toast(t("toast.localRemoved", { title: track.title }), "trash", {
      label: t("toast.undo"),
      run: () => api.trashRestore("local", track.sourceId).catch(fail),
    });
  } catch (e) {
    fail(e);
  }
}

