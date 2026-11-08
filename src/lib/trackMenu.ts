// The song menu (⋮ button and right-click). More entries arrive with playlists and likes.
import type { Track } from "./api";
import { t } from "./i18n/index.svelte";
import { menu, type MenuItem } from "./stores/menu.svelte";
import { player } from "./stores/player.svelte";
import { ui } from "./stores/ui.svelte";

export function trackMenuItems(tracks: Track[], index: number): MenuItem[] {
  const track = tracks[index];
  return [
    { label: t("menu.play"), icon: "play", action: () => player.playTracks(tracks, index) },
    {
      label: t("menu.playNext"),
      icon: "playNext",
      action: () => {
        player.playNextTracks([track]);
        ui.toast(t("toast.playingNext"), "playNext");
      },
    },
    {
      label: t("menu.addToQueue"),
      icon: "queue",
      action: () => {
        player.addTracks([track]);
        ui.toast(t("toast.addedToQueue"), "queue");
      },
    },
    {
      label: t("menu.copyLink"),
      icon: "link",
      disabled: !track.link,
      action: () => {
        if (!track.link) return;
        navigator.clipboard.writeText(track.link).then(() => ui.toast(t("toast.linkCopied"), "check"));
      },
    },
  ];
}

export function openTrackMenu(e: MouseEvent, tracks: Track[], index: number): void {
  menu.show(e, trackMenuItems(tracks, index));
}
