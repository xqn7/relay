<!-- Settings → About: version and updates (Relay itself and the YouTube helper, yt-dlp). -->
<script lang="ts">
  import { onMount } from "svelte";
  import { api, isAppError } from "../../lib/api";
  import logo from "../../lib/assets/logo.svg";
  import Button from "../../lib/components/Button.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { desktop } from "../../lib/stores/desktop.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import SettingRow from "./SettingRow.svelte";

  let version = $state("");
  let appBusy = $state(false);
  let appStatus = $state("");
  // undefined while yt-dlp is still being asked (it takes a second or two to answer).
  let ytdlp = $state<string | null | undefined>(undefined);
  let ytdlpBusy = $state(false);
  let ytdlpStatus = $state("");

  onMount(() => {
    api.desktopInfo().then((i) => (version = i.version)).catch(() => {});
    api.ytdlpVersion().then((v) => (ytdlp = v)).catch(() => (ytdlp = null));
  });

  const failure = (e: unknown) => (isAppError(e) && (e.code === "offline" || e.code === "network" || e.code === "timeout") ? t("error.offline") : t("update.failed"));

  async function checkApp() {
    appBusy = true;
    appStatus = "";
    try {
      const found = await desktop.checkForUpdate();
      appStatus = found ? "" : t("update.upToDate");
    } catch (e) {
      appStatus = failure(e);
    } finally {
      appBusy = false;
    }
  }

  async function updateYtdlp() {
    ytdlpBusy = true;
    ytdlpStatus = "";
    try {
      const [v, changed] = await api.ytdlpUpdate();
      ytdlp = v;
      ytdlpStatus = changed ? t("update.ytdlpUpdated") : t("update.upToDate");
    } catch (e) {
      ytdlpStatus = failure(e);
    } finally {
      ytdlpBusy = false;
    }
  }
</script>

<div class="head">
  <img src={logo} alt="" width="56" height="56" />
  <div>
    <h3>Relay</h3>
    {#if version}<p>{t("about.version", { version })}</p>{/if}
  </div>
</div>

<h3>{t("about.updates")}</h3>
<div class="card">
  {#if desktop.update}
    <SettingRow label={t("update.readyShort", { version: desktop.update.version })} desc={t("update.sub")}>
      <Button variant="primary" onclick={() => desktop.update && dialogs.open({ kind: "update", info: desktop.update })}>{t("update.details")}</Button>
    </SettingRow>
  {:else}
    <SettingRow label={t("about.app")} desc={appStatus || t("about.appDesc")}>
      <Button variant="secondary" busy={appBusy} onclick={checkApp}>{t("about.check")}</Button>
    </SettingRow>
  {/if}
  <SettingRow label={t("about.ytdlp")} desc={ytdlpStatus || (ytdlp === null ? t("about.ytdlpMissing") : t("about.ytdlpVersion", { version: ytdlp ?? "…" }))}>
    <Button variant="secondary" busy={ytdlpBusy} onclick={updateYtdlp}>{t("about.update")}</Button>
  </SettingRow>
</div>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 26px;
  }
  .head h3 {
    margin: 0;
    font-size: var(--fs-xl);
  }
  .head p {
    margin: 2px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-3);
  }
</style>
