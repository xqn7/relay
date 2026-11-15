<!-- Settings → About: version and updates (Relay itself and the YouTube helper, yt-dlp). -->
<script lang="ts">
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
    { label: "about.issues", desc: "about.issuesDesc", url: `${REPO}/issues` },
    { label: "about.releases", desc: "about.releasesDesc", url: `${REPO}/releases` },
  ] as const;

  let version = $state("");
  let dev = $state(false);
  let appBusy = $state(false);
  let appStatus = $state("");
  // undefined while yt-dlp is still being asked (it takes a second or two to answer).
  let ytdlp = $state<string | null | undefined>(undefined);
  let ytdlpBusy = $state(false);
  let ytdlpStatus = $state("");

  onMount(() => {
    api.desktopInfo().then((i) => ((version = i.version), (dev = i.dev))).catch(() => {});
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
  <img class="logo-mark" src={logo} alt="" width="56" height="56" />
  <div>
    <h3>{dev ? "Relay Dev" : "Relay"}</h3>
    {#if version}<p>{t("about.version", { version })}</p>{/if}
  </div>
</div>

<h3>{t("about.updates")}</h3>
<div class="card">
  {#if dev}
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
  <SettingRow label={t("about.ytdlp")} desc={ytdlpStatus || (ytdlp === null ? t("about.ytdlpMissing") : t("about.ytdlpVersion", { version: ytdlp ?? "…" }))}>
    <Button variant="secondary" busy={ytdlpBusy} onclick={updateYtdlp}>{t("about.update")}</Button>
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
