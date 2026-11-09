// Going to artist and album pages from any song, whatever service it came from.
import { api, IMPORT_ONLY, type ArtistRef, type SourceId, type Track } from "./api";
import { t } from "./i18n/index.svelte";
import { ui } from "./stores/ui.svelte";

/** The source a song's audio comes from ("none" when an imported song matched nothing). */
export function playsFrom(track: Track): SourceId | "none" {
  return track.via ?? track.source;
}

/** `source` is where the artist reference came from (a song's or album's source). */
export async function openArtist(artist: ArtistRef, source: SourceId): Promise<void> {
  if (artist.id && (source === "youtube" || source === "soundcloud")) {
    ui.open({ name: "artist", source, id: artist.id, artistName: artist.name });
    return;
  }
  // Imported and local songs only know the artist's name; find them on YouTube Music.
  try {
    const found = await api.artistFind(artist.name);
    if (found) ui.open({ name: "artist", source: found.source, id: found.id, artistName: found.name });
    else ui.toast(t("toast.artistNotFound", { name: artist.name }), "info");
  } catch {
    ui.toast(t("error.offline"), "alert");
  }
}

/** The album page for a song, when one can be found. */
export async function openAlbum(track: Track): Promise<void> {
  let target: Track | null = track;
  if (IMPORT_ONLY.has(track.source)) target = await api.matchGet(track).catch(() => null);
  const album = target?.album;
  if (target?.source === "youtube" && album?.id) ui.open({ name: "album", source: "youtube", id: album.id });
  else ui.toast(t("toast.noAlbum"), "info");
}

export function hasAlbumPage(track: Track): boolean {
  return (track.source === "youtube" && !!track.album?.id) || (IMPORT_ONLY.has(track.source) && track.via === "youtube");
}
