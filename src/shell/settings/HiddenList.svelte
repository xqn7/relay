<!-- Settings → Playback: songs and artists marked "Don't play", with a way back. -->
<script lang="ts">
  import type { Hidden } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { library } from "../../lib/stores/library.svelte";
  import { ui } from "../../lib/stores/ui.svelte";

  async function unhide(h: Hidden) {
    await library.setHidden(h.kind, h.value, h.label, false);
    ui.toast(t("hidden.allowed", { name: h.label }), "check", {
      label: t("toast.undo"),
      run: () => library.setHidden(h.kind, h.value, h.label, true),
    });
  }
</script>

{#if library.hidden.length === 0}
  <p class="empty">{t("hidden.empty")}</p>
{:else}
  {#each library.hidden as h (h.kind + h.value)}
    <div class="row">
      <Icon name={h.kind === "artist" ? "user" : "music"} size={16} />
      <div class="text">
        <div class="label truncate">{h.label}</div>
        <div class="kind">{h.kind === "artist" ? t("hidden.artist") : t("hidden.song")}</div>
      </div>
      <Button variant="ghost" onclick={() => unhide(h)}>{t("hidden.allow")}</Button>
    </div>
  {/each}
{/if}

<style>
  .empty {
    margin: 0;
    padding: 14px 16px;
    font-size: var(--fs-sm);
    color: var(--text-3);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 8px 8px 16px;
    color: var(--text-2);
  }
  .row + .row {
    border-top: 1px solid var(--hairline);
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .label {
    font-size: var(--fs-sm);
    color: var(--text);
  }
  .kind {
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
</style>
