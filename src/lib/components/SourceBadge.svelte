<script lang="ts">
  import type { SourceId } from "../api";
  import { t } from "../i18n/index.svelte";
  import Icon from "./Icon.svelte";

  let { source }: { source: SourceId } = $props();
</script>

<span class="badge" title={t(source === "soundcloud" ? "source.soundcloud" : "source.youtube")}>
  <Icon name={source === "soundcloud" ? "cloud" : "youtube"} size={11} strokeWidth={2.4} />
</span>

<style>
  .badge {
    display: inline-grid;
    place-items: center;
    flex: none;
    width: 16px;
    height: 14px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.1);
    color: var(--text-2);
  }
</style>
