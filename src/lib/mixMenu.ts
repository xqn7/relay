// Actions for a daily mix (right-click on its tile, buttons on its page).
import type { Mix } from "./api";
import { t } from "./i18n/index.svelte";
import { dialogs } from "./stores/dialogs.svelte";
import { library } from "./stores/library.svelte";
import { menu } from "./stores/menu.svelte";
import { player } from "./stores/player.svelte";
import { ui } from "./stores/ui.svelte";

export function playMix(m: Mix, shuffle: boolean): void {
  if (m.tracks.length === 0) return;
  if (player.queue.shuffle !== shuffle) player.toggleShuffle();
  player.playTracks(m.tracks, shuffle ? Math.floor(Math.random() * m.tracks.length) : 0);
}

/** Keeps today's mix as a normal playlist (mixes change every day). */
export function saveMix(m: Mix): void {
  dialogs.open({
    kind: "prompt",
    title: t("mix.save"),
    label: t("playlist.name"),
    value: t("mix.title", { n: m.number }),
    confirm: t("playlist.create"),
    onConfirm: async (name) => {
      const id = await library.createPlaylist(name, m.tracks);
      if (id) ui.toast(t("toast.addedToPlaylist", { name, n: m.tracks.length }), "check", { label: t("toast.open"), run: () => ui.open({ name: "playlist", id }) });
    },
  });
}

export function mixMenu(e: MouseEvent, m: Mix): void {
  menu.show(e, [
    { label: t("menu.play"), icon: "play", action: () => playMix(m, false) },
    { label: t("playlist.shuffle"), icon: "shuffle", action: () => playMix(m, true) },
    {
      label: t("menu.addToQueue"),
      icon: "queue",
      action: () => {
        player.addTracks(m.tracks);
        ui.toast(t("toast.addedToQueue"), "queue");
      },
    },
    { label: t("mix.save"), icon: "listPlus", divider: true, action: () => saveMix(m) },
  ]);
}
