<!-- Settings → Cache & Storage: what Relay keeps on this PC, the limits, the folders, backups
     and the connection check. -->
<script lang="ts">
  import { api, formatBytes, type Usage } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import Toggle from "../../lib/components/Toggle.svelte";
  import type { Key } from "../../lib/i18n/en";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { fail } from "../../lib/stores/library.svelte";
  import { offline } from "../../lib/stores/offline.svelte";
  import { player } from "../../lib/stores/player.svelte";
  import { CACHE_SIZES, DOWNLOAD_SIZES, settings } from "../../lib/stores/settings.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import SettingRow from "./SettingRow.svelte";

  let { visible }: { visible: (label: Key, desc: Key) => boolean } = $props();
  const v = $derived(settings.values);
  let usage = $state<Usage | null>(null);
  let folder = $state("");
  let timer = 0;

  function loadUsage() {
    clearTimeout(timer);
    timer = window.setTimeout(() => api.storageUsage().then((u) => (usage = u)).catch((e) => console.error(e)), 150);
  }

  $effect(() => {
    // Sizes change whenever a song is cached, downloaded or removed.
    void offline.downloaded;
    void offline.cached;
    void offline.status.done;
    loadUsage();
  });

  $effect(() => {
    api.localFolder().then((f) => (folder = f)).catch(() => {});
  });

  const myMusic = $derived(usage ? usage.downloadsBytes + usage.importedBytes : 0);
  const cache = $derived(usage ? usage.cacheSongsBytes + usage.artworkBytes : 0);
  const total = $derived(myMusic + cache);

  function sizeOptions(sizes: number[]) {
    return sizes.map((gb) => ({ value: gb, label: gb === 0 ? t("storage.noLimit") : `${gb} GB` }));
  }

  function songsFor(gb: number): string {
    if (gb === 0 || !usage) return t("storage.noLimitDesc");
    return t("storage.aboutSongs", { n: Math.floor((gb * 1024 ** 3) / usage.averageSongBytes).toLocaleString() });
  }

  const downloadLine = $derived.by(() => {
    const s = offline.status;
    if (s.full) return t("storage.full");
    if (s.active) return t("storage.downloading", { done: s.done, total: s.total });
    if (s.total > 0 && !offline.online && s.done < s.total) return t("storage.waitingOnline");
    return null;
  });

  async function changeFolder() {
    try {
      const picked = await api.localFolderPick();
      if (picked) folder = picked;
    } catch (e) {
      fail(e);
    }
  }

  function clear(what: "songs" | "artwork" | "downloads") {
    const run = () =>
      api
        .storageClear(what)
        .then(() => {
          ui.toast(t("storage.cleared"), "check");
          loadUsage();
        })
        .catch(fail);
    if (what !== "downloads") return run();
    dialogs.open({ kind: "confirm", title: t("storage.removeDownloadsTitle"), body: t("storage.removeDownloadsBody"), confirm: t("storage.remove"), onConfirm: run });
  }

  async function backUp() {
    try {
      if (await api.backupCreate()) ui.toast(t("backup.saved"), "check");
    } catch (e) {
      fail(e);
    }
  }

  function restore() {
    dialogs.open({
      kind: "confirm",
      title: t("backup.restoreTitle"),
      body: t("backup.restoreBody"),
      confirm: t("backup.chooseFile"),
      onConfirm: async () => {
        try {
          const n = await api.backupRestore();
          if (n === 0) return;
          // The library under the signed-in account may be a different one now: start fresh.
          await player.reset();
          ui.toast(t("backup.restored", { n }), "check");
          setTimeout(() => location.reload(), 900);
        } catch (e) {
          console.error(e);
          ui.toast(t("backup.badFile"), "alert");
        }
      },
    });
  }

  async function pickBackupFolder() {
    try {
      const picked = await api.backupFolderPick();
      if (picked) settings.set("backupFolder", picked);
      if (picked && !settings.values.autoBackup) settings.set("autoBackup", true);
    } catch (e) {
      fail(e);
    }
  }

  function toggleAutoBackup(on: boolean) {
    if (on && !settings.values.backupFolder) {
      pickBackupFolder();
      return;
    }
    settings.set("autoBackup", on);
  }
</script>

{#if visible("storage.used", "storage.usedDesc")}
  <div class="card overview">
    <div class="total">
      <span class="big">{usage ? formatBytes(total) : "…"}</span>
      <span class="of">{t("storage.used")}</span>
    </div>
    <div class="bar" role="img" aria-label={t("storage.usedDesc")}>
      {#if total > 0}
        <span class="mine" style:width="{(myMusic / total) * 100}%"></span>
        <span class="cache" style:width="{(cache / total) * 100}%"></span>
      {/if}
    </div>
    <div class="legend">
      <span><i class="dot mine"></i>{t("storage.myMusic")} · {formatBytes(myMusic)}</span>
      <span><i class="dot cache"></i>{t("storage.cache")} · {formatBytes(cache)}</span>
      {#if usage?.freeBytes != null}<span class="free">{t("storage.free", { size: formatBytes(usage.freeBytes) })}</span>{/if}
    </div>
  </div>
{/if}

{#if visible("storage.likedDownload", "storage.likedDownloadDesc") || visible("storage.downloadSpace", "storage.downloadSpaceDesc")}
  <h3>{t("storage.downloads")}</h3>
  <div class="card">
    {#if visible("storage.likedDownload", "storage.likedDownloadDesc")}
      <SettingRow label={t("storage.likedDownload")} desc={`${t("storage.likedDownloadDesc")} · ${formatBytes(usage?.likedBytes ?? 0)}`}>
        <Toggle label={t("storage.likedDownload")} checked={v.autoDownloadLiked} onchange={(x) => settings.set("autoDownloadLiked", x)} />
      </SettingRow>
    {/if}
    {#if visible("storage.imported", "storage.importedDesc")}
      <SettingRow label={t("storage.imported")} desc={`${t("storage.importedDesc")} · ${formatBytes(usage?.importedBytes ?? 0)}`}>
        <Button variant="ghost" icon="folder" onclick={() => api.localOpenFolder().catch(fail)}>{t("settings.open")}</Button>
      </SettingRow>
    {/if}
    {#if visible("storage.downloadSpace", "storage.downloadSpaceDesc")}
      <SettingRow label={t("storage.downloadSpace")} desc={`${t("storage.downloadSpaceDesc")} ${songsFor(v.downloadLimitGb)}`}>
        <Segmented label={t("storage.downloadSpace")} value={v.downloadLimitGb} options={sizeOptions(DOWNLOAD_SIZES)} onchange={(x) => settings.set("downloadLimitGb", x)} />
      </SettingRow>
    {/if}
    <SettingRow label={t("storage.saved", { n: usage?.downloadsCount ?? 0 })} desc={downloadLine ?? formatBytes(usage?.downloadsBytes ?? 0)}>
      <Button variant="ghost" icon="folder" onclick={() => api.storageOpen("downloads").catch(fail)}>{t("settings.open")}</Button>
      <Button variant="secondary" disabled={!usage?.downloadsCount} onclick={() => clear("downloads")}>{t("storage.remove")}</Button>
    </SettingRow>
  </div>
{/if}

{#if visible("storage.maxCache", "storage.maxCacheDesc") || visible("storage.showCached", "storage.showCachedDesc")}
  <h3>{t("storage.cache")}</h3>
  <div class="card">
    <SettingRow label={t("storage.cachedSongs", { n: usage?.cacheSongsCount ?? 0 })} desc={formatBytes(usage?.cacheSongsBytes ?? 0)}>
      <Button variant="ghost" icon="folder" onclick={() => api.storageOpen("cache").catch(fail)}>{t("settings.open")}</Button>
      <Button variant="secondary" disabled={!usage?.cacheSongsCount} onclick={() => clear("songs")}>{t("storage.clear")}</Button>
    </SettingRow>
    <SettingRow label={t("storage.artwork")} desc={formatBytes(usage?.artworkBytes ?? 0)}>
      <Button variant="secondary" disabled={!usage?.artworkBytes} onclick={() => clear("artwork")}>{t("storage.clear")}</Button>
    </SettingRow>
    {#if visible("storage.maxCache", "storage.maxCacheDesc")}
      <SettingRow label={t("storage.maxCache")} desc={`${t("storage.maxCacheDesc")} ${songsFor(v.maxCacheGb)}`}>
        <Segmented label={t("storage.maxCache")} value={v.maxCacheGb} options={sizeOptions(CACHE_SIZES)} onchange={(x) => settings.set("maxCacheGb", x)} />
      </SettingRow>
    {/if}
    {#if visible("storage.showCached", "storage.showCachedDesc")}
      <SettingRow label={t("storage.showCached")} desc={t("storage.showCachedDesc")}>
        <Toggle label={t("storage.showCached")} checked={v.showCachedInLibrary} onchange={(x) => settings.set("showCachedInLibrary", x)} />
      </SettingRow>
    {/if}
  </div>
{/if}

{#if visible("settings.localFolder", "settings.localFolderDesc")}
  <h3>{t("settings.section.localMusic")}</h3>
  <div class="card">
    <SettingRow label={t("settings.localFolder")} desc={t("settings.localFolderDesc")}>
      <Button variant="ghost" icon="folder" onclick={() => api.localOpenFolder().catch(fail)}>{t("settings.open")}</Button>
      <Button variant="secondary" onclick={changeFolder}>{t("settings.change")}</Button>
    </SettingRow>
    <div class="path" title={folder}>{folder}</div>
  </div>
{/if}

{#if visible("backup.title", "backup.desc") || visible("backup.auto", "backup.autoDesc")}
  <h3>{t("backup.title")}</h3>
  <div class="card">
    <SettingRow label={t("backup.title")} desc={t("backup.desc")}>
      <Button variant="secondary" icon="upload" onclick={backUp}>{t("backup.create")}</Button>
      <Button variant="ghost" icon="download" onclick={restore}>{t("backup.restore")}</Button>
    </SettingRow>
    <SettingRow label={t("backup.auto")} desc={t("backup.autoDesc")}>
      <Toggle label={t("backup.auto")} checked={v.autoBackup} onchange={toggleAutoBackup} />
    </SettingRow>
    {#if v.backupFolder}
      <div class="path-row">
        <div class="path" title={v.backupFolder}>{v.backupFolder}</div>
        <Button variant="ghost" onclick={pickBackupFolder}>{t("settings.change")}</Button>
      </div>
    {/if}
  </div>
{/if}

{#if visible("connection.title", "connection.desc")}
  <h3>{t("connection.section")}</h3>
  <div class="card">
    <SettingRow label={t("connection.title")} desc={t("connection.desc")}>
      <Button variant="secondary" icon="globe" onclick={() => dialogs.open({ kind: "connection" })}>{t("connection.run")}</Button>
    </SettingRow>
  </div>
{/if}

<style>
  h3 {
    margin: 22px 2px 10px;
    font-size: var(--fs-md);
    font-weight: 700;
  }
  .card {
    border-radius: var(--r-lg);
    background: rgba(255, 255, 255, 0.05);
  }
  .overview {
    padding: 16px;
  }
  .total {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .big {
    font-size: var(--fs-2xl);
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .of {
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .bar {
    display: flex;
    gap: 2px;
    height: 8px;
    margin: 12px 0 10px;
    overflow: hidden;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.08);
  }
  .bar span {
    min-width: 2px;
    transition: width var(--t) var(--ease);
  }
  .mine {
    background: rgba(255, 255, 255, 0.9);
  }
  .cache {
    background: rgba(255, 255, 255, 0.38);
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
    font-size: var(--fs-xs);
    color: var(--text-2);
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .free {
    margin-left: auto;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
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
  .path-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding-right: 12px;
  }
  .path-row .path {
    flex: 1;
    padding-bottom: 0;
  }
  .path-row {
    padding-bottom: 10px;
  }
</style>
