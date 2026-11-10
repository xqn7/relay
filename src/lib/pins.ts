// Speed dial pins for catalog items, and the right-click menus that offer "Pin to Home".
import type { ArtistCard, Collection, Pin, PlaylistSummary, SearchSource } from "./api";
import { menu } from "./stores/menu.svelte";
import { speedDial } from "./stores/speedDial.svelte";

/** Only web covers are kept with a pin (local art paths belong to this PC). */
const webCover = (url: string | null): string | null => (url?.startsWith("https://") ? url : null);

export function playlistPin(p: PlaylistSummary): Pin {
  // Playlists show their live name and cover on Home, so neither needs storing.
  return { kind: "playlist", source: "", id: p.id, title: p.name, subtitle: "", cover: null };
}

export function albumPin(source: SearchSource, id: string, title: string, subtitle: string, cover: string | null): Pin {
  return { kind: "album", source, id, title, subtitle, cover: webCover(cover) };
}

export function artistPin(a: Pick<ArtistCard, "source" | "id" | "name" | "avatar">, subtitle = ""): Pin {
  return { kind: "artist", source: a.source, id: a.id, title: a.name, subtitle, cover: webCover(a.avatar) };
}

/** Albums (not single SoundCloud songs or playlists) can be pinned from their tile. */
export function collectionMenu(e: MouseEvent, c: Collection): void {
  if (c.kind !== "album") return;
  menu.show(e, [speedDial.menuItem(albumPin(c.source, c.id, c.title, c.subtitle, c.cover))]);
}

export function artistMenu(e: MouseEvent, a: Pick<ArtistCard, "source" | "id" | "name" | "avatar">): void {
  menu.show(e, [speedDial.menuItem(artistPin(a))]);
}
