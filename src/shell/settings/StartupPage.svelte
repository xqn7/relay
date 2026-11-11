<!-- Settings → Startup: start with Windows (hidden in the tray) and whether to carry on playing. -->
<script lang="ts">
  import Toggle from "../../lib/components/Toggle.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { settings } from "../../lib/stores/settings.svelte";
  import SettingRow from "./SettingRow.svelte";

  let { visible }: { visible: (label: Key, desc: Key) => boolean } = $props();
  const v = $derived(settings.values);
</script>

{#if visible("startup.withWindows", "startup.withWindowsDesc") || visible("startup.play", "startup.playDesc")}
  <h3>{t("settings.page.startup")}</h3>
  <div class="card">
    <SettingRow label={t("startup.withWindows")} desc={t("startup.withWindowsDesc")}>
      <Toggle label={t("startup.withWindows")} checked={v.startWithWindows} onchange={(x) => settings.set("startWithWindows", x)} />
    </SettingRow>
    {#if v.startWithWindows}
      <SettingRow label={t("startup.play")} desc={t("startup.playDesc")}>
        <Toggle label={t("startup.play")} checked={v.startPlay} onchange={(x) => settings.set("startPlay", x)} />
      </SettingRow>
    {/if}
  </div>
{/if}
