// Settings pages and their searchable rows. Search matches page names and every row's text.
import type { Key } from "../../lib/i18n/en";
import type { IconName } from "../../lib/icons";

export type PageId = "playback" | "appearance" | "miniPlayer" | "youtube" | "discord" | "storage" | "startup" | "about" | "docs";

export interface PageDef {
  id: PageId;
  group: Key;
  icon: IconName;
  label: Key;
  /** Label/description key pairs of the rows on this page, for search. */
  rows: [Key, Key][];
}

export const pages: PageDef[] = [
  {
    id: "playback",
    group: "settings.group.player",
    icon: "play",
    label: "settings.page.playback",
    rows: [
      ["settings.autoplay", "settings.autoplayDesc"],
      ["settings.doubleClick", "settings.doubleClickDesc"],
      ["settings.startup", "settings.startupDesc"],
      ["settings.pauseOnDevice", "settings.pauseOnDeviceDesc"],
      ["settings.gameMode", "settings.gameModeDesc"],
      ["settings.crossfade", "settings.crossfadeDesc"],
      ["settings.normalize", "settings.normalizeDesc"],
      ["settings.quality", "settings.qualityDesc"],
      ["sound.output", "sound.outputDesc"],
      ["eq.title", "eq.desc"],
      ["sound.speedPitch", "sound.speedPitchDesc"],
      ["settings.sponsorBlock", "settings.sponsorBlockDesc"],
      ["settings.hidden", "settings.hiddenDesc"],
    ],
  },
  {
    id: "appearance",
    group: "settings.group.interface",
    icon: "palette",
    label: "settings.page.appearance",
    rows: [
      ["quality.title", "quality.desc"],
      ["bg.mode", "bg.modeDesc"],
      ["bg.mix", "bg.mixDesc"],
      ["settings.lyricsStyle", "settings.lyricsStyleDesc"],
      ["settings.translateTo", "settings.translateToDesc"],
      ["settings.deepl", "settings.deeplDesc"],
      ["settings.upNext", "settings.upNextDesc"],
      ["settings.idle", "settings.idleDesc"],
      ["settings.waveform", "settings.waveformDesc"],
      ["settings.visualizer", "settings.visualizerDesc"],
      ["desktopLyrics.title", "desktopLyrics.desc"],
      ["lockscreen.title", "lockscreen.desc"],
    ],
  },
  {
    id: "miniPlayer",
    group: "settings.group.interface",
    icon: "overlay",
    label: "settings.page.miniPlayer",
    rows: [
      ["mini.enable", "mini.enableDesc"],
      ["mini.position", "mini.positionDesc"],
      ["mini.shortcut", "mini.shortcutDesc"],
      ["mini.autoShow", "mini.autoShowDesc"],
      ["mini.onTop", "mini.onTopDesc"],
    ],
  },
  {
    id: "youtube",
    group: "settings.group.integrations",
    icon: "youtube",
    label: "settings.page.youtube",
    rows: [
      ["youtube.useAccount", "youtube.useAccountDesc"],
      ["youtube.ask", "youtube.askDesc"],
      ["youtube.sync", "youtube.syncDesc"],
    ],
  },
  {
    id: "discord",
    group: "settings.group.integrations",
    icon: "chat",
    label: "settings.page.discord",
    rows: [
      ["discord.enable", "discord.enableDesc"],
      ["discord.format", "discord.formatDesc"],
      ["discord.timeline", "discord.timelineDesc"],
      ["discord.artwork", "discord.artworkDesc"],
      ["discord.badge", "discord.badgeDesc"],
      ["discord.paused", "discord.pausedDesc"],
      ["discord.buttons", "discord.buttonsDesc"],
    ],
  },
  {
    id: "storage",
    group: "settings.group.application",
    icon: "database",
    label: "settings.page.storage",
    rows: [
      ["storage.used", "storage.usedDesc"],
      ["storage.likedDownload", "storage.likedDownloadDesc"],
      ["storage.imported", "storage.importedDesc"],
      ["storage.downloadSpace", "storage.downloadSpaceDesc"],
      ["storage.maxCache", "storage.maxCacheDesc"],
      ["storage.showCached", "storage.showCachedDesc"],
      ["storage.lyrics", "storage.lyricsDesc"],
      ["settings.localFolder", "settings.localFolderDesc"],
      ["backup.title", "backup.desc"],
      ["backup.auto", "backup.autoDesc"],
      ["connection.title", "connection.desc"],
    ],
  },
  {
    id: "startup",
    group: "settings.group.application",
    icon: "power",
    label: "settings.page.startup",
    rows: [
      ["startup.withWindows", "startup.withWindowsDesc"],
      ["startup.play", "startup.playDesc"],
    ],
  },
  { id: "about", group: "settings.group.application", icon: "info", label: "settings.page.about", rows: [["about.app", "about.appDesc"], ["about.ytdlp", "about.ytdlpMissing"]] },
  { id: "docs", group: "settings.group.help", icon: "book", label: "settings.page.docs", rows: [] },
];
