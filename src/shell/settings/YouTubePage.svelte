<!-- Settings → YouTube: optional sign-in (Google's own login page in a private window) and
     what the login is used for. -->
<script lang="ts">
  import Button from "../../lib/components/Button.svelte";
  import Toggle from "../../lib/components/Toggle.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { settings } from "../../lib/stores/settings.svelte";
  import { youtube } from "../../lib/stores/youtube.svelte";
  import SettingRow from "./SettingRow.svelte";

  let { visible }: { visible: (label: Key, desc: Key) => boolean } = $props();
  const v = $derived(settings.values);
  const active = $derived(youtube.signedIn && !youtube.expired);
  const status = $derived(youtube.expired ? t("youtube.expired") : youtube.signedIn ? t("youtube.statusIn") : t("youtube.statusOut"));

  function setSync(on: boolean) {
    settings.set("youtubeSyncLibrary", on);
    youtube.loadPlaylists();
  }
</script>

<h3>{t("youtube.account")}</h3>
<div class="card">
  <SettingRow label={status} desc={active ? t("youtube.statusInDesc") : t("youtube.statusOutDesc")}>
    <div class="buttons">
      {#if !active}<Button variant="primary" onclick={() => youtube.signIn()}>{t("youtube.signIn")}</Button>{/if}
      {#if youtube.signedIn}<Button variant="secondary" onclick={() => youtube.signOut()}>{t("youtube.signOut")}</Button>{/if}
    </div>
  </SettingRow>
  <p class="note">{t("youtube.warning")}</p>
</div>

{#if visible("youtube.useAccount", "youtube.useAccountDesc") || visible("youtube.ask", "youtube.askDesc") || visible("youtube.sync", "youtube.syncDesc")}
  <h3>{t("youtube.uses")}</h3>
  <div class="card">
    {#if visible("youtube.useAccount", "youtube.useAccountDesc")}
      <SettingRow label={t("youtube.useAccount")} desc={t("youtube.useAccountDesc")}>
        <Toggle label={t("youtube.useAccount")} checked={v.youtubeUseAccount} onchange={(x) => settings.set("youtubeUseAccount", x)} />
      </SettingRow>
    {/if}
    {#if visible("youtube.ask", "youtube.askDesc")}
      <SettingRow label={t("youtube.ask")} desc={t("youtube.askDesc")}>
        <Toggle label={t("youtube.ask")} checked={v.youtubeAskSignIn} onchange={(x) => settings.set("youtubeAskSignIn", x)} />
      </SettingRow>
    {/if}
    {#if visible("youtube.sync", "youtube.syncDesc")}
      <SettingRow label={t("youtube.sync")} desc={t("youtube.syncDesc")}>
        <Toggle label={t("youtube.sync")} checked={v.youtubeSyncLibrary} onchange={setSync} />
      </SettingRow>
    {/if}
  </div>
{/if}

<style>
  .buttons {
    display: flex;
    gap: 8px;
  }
</style>
