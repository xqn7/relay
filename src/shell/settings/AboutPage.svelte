<!-- Settings → About: version and updates (Relay itself and the YouTube helper, yt-dlp). -->
<script lang="ts">
  import { APP_NAME, DEV_BUILD } from "../../lib/brand";
  import { onMount } from "svelte";
  import { api, isAppError } from "../../lib/api";
  import logo from "../../lib/assets/logo.svg";
  import Button from "../../lib/components/Button.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { desktop } from "../../lib/stores/desktop.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import SettingRow from "./SettingRow.svelte";

  const REPO = "https://github.com/xqn7/relay";
  const links = [
    { label: "about.repo", desc: "about.repoDesc", url: REPO },
    { label: "about.releases", desc: "about.releasesDesc", url: `${REPO}/releases` },
  ] as const;

  let version = $state("");
  let appBusy = $state(false);
  let appStatus = $state("");

  onMount(() => {
    api.desktopInfo().then((i) => (version = i.version)).catch(() => {});
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

</script>

<div class="head">
  <img class="logo-mark" src={logo} alt="" width="56" height="56" />
  <div>
    <h3>{APP_NAME}</h3>
    {#if version}<p>{t("about.version", { version })}</p>{/if}
  </div>
</div>

<h3>{t("about.updates")}</h3>
<div class="card">
  {#if DEV_BUILD}
    <SettingRow label={t("about.app")} desc={t("about.devUpdates")} />
  {:else if desktop.update}
    <SettingRow label={t("update.readyShort", { version: desktop.update.version })} desc={t("update.sub")}>
      <Button variant="primary" onclick={() => desktop.update && dialogs.open({ kind: "update", info: desktop.update })}>{t("update.details")}</Button>
    </SettingRow>
  {:else}
    <SettingRow label={t("about.app")} desc={appStatus || t("about.appDesc")}>
      <Button variant="secondary" busy={appBusy} onclick={checkApp}>{t("about.check")}</Button>
    </SettingRow>
  {/if}
</div>

<h3>{t("about.help")}</h3>
<div class="card">
  <SettingRow label={t("problem.title")} desc={t("about.problemDesc")}>
    <Button variant="secondary" onclick={() => dialogs.open({ kind: "problem" })}>{t("about.problemSend")}</Button>
  </SettingRow>
</div>

<h3>{t("about.links")}</h3>
<div class="card">
  {#each links as link (link.url)}
    <button class="link" onclick={() => api.openExternal(link.url)}>
      <span class="names">
        <span class="label">{t(link.label)}</span>
        <span class="desc">{t(link.desc)}</span>
      </span>
      <Icon name="external" size={16} />
    </button>
  {/each}
</div>

<p class="legal">{t("about.license")}</p>

<style>
  .link {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s4);
    width: 100%;
    padding: 12px 16px;
    border: 0;
    background: none;
    text-align: left;
    color: var(--text-3);
  }
  .link:first-child {
    border-radius: var(--r-lg) var(--r-lg) 0 0;
  }
  .link:last-child {
    border-radius: 0 0 var(--r-lg) var(--r-lg);
  }
  .link + .link {
    border-top: 1px solid rgba(var(--ink), 0.04);
  }
  .link:hover {
    background: var(--hover);
    color: var(--text);
  }
  .names {
    display: grid;
    gap: 3px;
  }
  .label {
    font-size: var(--fs-md);
    font-weight: 600;
    color: var(--text);
  }
  .desc {
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .legal {
    margin: 18px 2px 0;
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--text-3);
  }
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
