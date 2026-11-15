<script lang="ts">
  import type { SourceId } from "../api";
  import { t } from "../i18n/index.svelte";
  import type { IconName } from "../icons";
  import Icon from "./Icon.svelte";

  /** The source the song plays from; "none" for imported songs nothing matched. */
  let { source }: { source: SourceId | "none" } = $props();

  const look = $derived.by((): { icon: IconName; label: string } => {
    switch (source) {
      case "soundcloud":
        return { icon: "cloud", label: t("source.soundcloud") };
      case "local":
        return { icon: "folder", label: t("source.local") };
      case "none":
        return { icon: "alert", label: t("source.none") };
      case "spotify":
      case "deezer":
      case "apple":
        // Imported, not matched yet: the match is searched when it first plays.
        return { icon: "music", label: t("source.notMatched") };
      default:
        return { icon: "youtube", label: t("source.youtube") };
    }
  });
</script>

<span class="badge" class:warn={source === "none"} title={look.label}>
  <Icon name={look.icon} size={11} strokeWidth={2.4} />
</span>

<style>
  .badge {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: 16px;
    height: 14px;
    border-radius: 3px;
    background: rgba(var(--ink), 0.1);
    color: var(--text-2);
  }
  .warn {
    background: rgba(var(--ink), 0.18);
    color: var(--text);
  }
</style>
