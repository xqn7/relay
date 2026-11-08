// Settings pages and their searchable rows. Search matches page names and every row's text.
import type { Key } from "../../lib/i18n/en";
import type { IconName } from "../../lib/icons";

export type PageId = "playback" | "appearance" | "miniPlayer" | "youtube" | "discord" | "storage" | "about" | "docs";

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
      ["settings.crossfade", "settings.crossfadeDesc"],
      ["settings.normalize", "settings.normalizeDesc"],
      ["settings.quality", "settings.qualityDesc"],
    ],
  },
  { id: "appearance", group: "settings.group.interface", icon: "palette", label: "settings.page.appearance", rows: [] },
  { id: "miniPlayer", group: "settings.group.interface", icon: "overlay", label: "settings.page.miniPlayer", rows: [] },
  { id: "youtube", group: "settings.group.integrations", icon: "youtube", label: "settings.page.youtube", rows: [] },
  { id: "discord", group: "settings.group.integrations", icon: "chat", label: "settings.page.discord", rows: [] },
  { id: "storage", group: "settings.group.application", icon: "database", label: "settings.page.storage", rows: [] },
  { id: "about", group: "settings.group.application", icon: "info", label: "settings.page.about", rows: [] },
  { id: "docs", group: "settings.group.help", icon: "book", label: "settings.page.docs", rows: [] },
];
