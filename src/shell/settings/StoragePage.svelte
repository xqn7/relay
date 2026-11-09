<!-- Settings → Cache & Storage. Only the Local music folder exists so far; cache and
     download settings arrive with offline playback. -->
<script lang="ts">
  import { api } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { fail } from "../../lib/stores/library.svelte";
  import SettingRow from "./SettingRow.svelte";

  let { visible }: { visible: (label: Key, desc: Key) => boolean } = $props();
  let folder = $state("");

  $effect(() => {
    api.localFolder().then((f) => (folder = f)).catch(() => {});
  });

  async function change() {
    try {
      const picked = await api.localFolderPick();
      if (picked) folder = picked;
    } catch (e) {
      fail(e);
    }
  }
</script>

{#if visible("settings.localFolder", "settings.localFolderDesc")}
  <h3>{t("settings.section.localMusic")}</h3>
  <div class="card">
    <SettingRow label={t("settings.localFolder")} desc={t("settings.localFolderDesc")}>
      <div class="actions">
        <Button variant="ghost" icon="folder" onclick={() => api.localOpenFolder().catch(fail)}>{t("settings.open")}</Button>
        <Button variant="secondary" onclick={change}>{t("settings.change")}</Button>
      </div>
    </SettingRow>
    <div class="path" title={folder}>{folder}</div>
  </div>
{/if}
<p class="later">{t("settings.storageLater")}</p>

<style>
  h3 {
    margin: 0 2px 10px;
    font-size: var(--fs-md);
    font-weight: 700;
  }
  .card {
    border-radius: var(--r-lg);
    background: rgba(255, 255, 255, 0.05);
  }
  .actions {
    display: flex;
    gap: 6px;
  }
  .path {
    padding: 0 16px 14px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: var(--fs-xs);
    color: var(--text-3);
    direction: rtl;
    text-align: left;
  }
  .later {
    margin: 18px 2px 0;
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
</style>
