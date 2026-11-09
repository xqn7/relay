<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { api, type RemoteList } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Cover from "../../lib/components/Cover.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import Spinner from "../../lib/components/Spinner.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { imports, openReport } from "../../lib/stores/imports.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import ImportProgress from "./ImportProgress.svelte";
  import { errorText, serviceName } from "./importText";

  let { url: initial = "" }: { url?: string } = $props();

  let url = $state(untrack(() => initial));
  let preview = $state<RemoteList | null>(null);
  let loading = $state(false);
  let error = $state("");
  let jobId = $state<string | null>(null);
  let input = $state<HTMLInputElement>();
  let seq = 0;

  const job = $derived(imports.get(jobId));
  const result = $derived(job?.result ?? null);

  $effect(() => {
    input?.focus();
    if (initial) load();
  });

  onDestroy(() => {
    if (imports.watched === jobId) imports.watched = null;
  });

  async function load() {
    const value = url.trim();
    if (!value) return;
    const mine = ++seq;
    loading = true;
    error = "";
    preview = null;
    try {
      const list = await api.importPreview(value);
      if (mine === seq) preview = list;
    } catch (e) {
      if (mine === seq) error = errorText(e);
    } finally {
      if (mine === seq) loading = false;
    }
  }

  async function start() {
    if (!preview) return;
    try {
      const id = await api.importStart(url.trim());
      imports.add(id, preview.title);
      imports.watched = id;
      jobId = id;
    } catch (e) {
      error = errorText(e);
    }
  }

  async function pickM3u() {
    const path = await api.importM3uPick().catch(() => null);
    if (path) {
      url = path;
      load();
    }
  }

  function close() {
    // An import keeps running in the background; its progress moves to a toast.
    if (jobId && !result) ui.toast(t("import.continues"), "download");
    dialogs.close();
  }

  function onPaste() {
    // Pasting a link is the whole intent: read it right away.
    setTimeout(load, 0);
  }
</script>

<Dialog title={t("import.title")} sub={t("import.sub")} width={520} onclose={close}>
  {#if !jobId}
    <form
      class="row"
      onsubmit={(e) => {
        e.preventDefault();
        load();
      }}>
      <label class="field">
        <Icon name="link" size={16} />
        <input bind:this={input} bind:value={url} placeholder={t("import.placeholder")} spellcheck="false" onpaste={onPaste} />
      </label>
      <Button variant="secondary" type="submit" busy={loading} disabled={!url.trim()}>{t("import.read")}</Button>
    </form>
    <div class="services">{t("import.services")}</div>

    {#if error}
      <p class="error"><Icon name="alert" size={15} />{error}</p>
    {/if}

    {#if loading}
      <div class="loading"><Spinner size={18} />{t("import.loadingPreview")}</div>
    {:else if preview}
      <div class="preview">
        <Cover src={preview.cover} size={96} radius={10} fallback="list" />
        <div class="meta">
          <span class="kind">{serviceName(preview.service)} · {t(preview.kind === "album" ? "import.kindAlbum" : preview.kind === "track" ? "import.kindTrack" : "import.kindPlaylist")}</span>
          <h3 class="truncate">{preview.title}</h3>
          {#if preview.owner}<span class="owner truncate">{preview.owner}</span>{/if}
          <span class="count">{t("import.songCount", { n: preview.tracks.length })}</span>
        </div>
      </div>
      {#if preview.truncated}
        <p class="note"><Icon name="info" size={15} />{t(preview.service === "spotify" ? "import.cutOffSpotify" : "import.cutOff", { n: preview.tracks.length })}</p>
      {/if}
      {#if preview.skipped.length}
        <p class="note"><Icon name="info" size={15} />{t("import.skippedNote", { n: preview.skipped.length })}</p>
      {/if}
    {/if}
  {:else if job}
    <ImportProgress {job} />
  {/if}

  {#snippet footer()}
    {#if !jobId}
      <Button variant="ghost" icon="upload" onclick={pickM3u}>{t("import.m3u")}</Button>
      <Button variant="ghost" icon="heart" onclick={() => dialogs.open({ kind: "importLiked" })}>{t("import.likedLink")}</Button>
      <span class="spacer"></span>
      <Button variant="primary" icon="download" disabled={!preview || loading} onclick={start}>{t("import.start")}</Button>
    {:else if result}
      {#if result.ok && result.unmatched.length + result.skipped.length > 0}
        <Button variant="secondary" onclick={() => openReport(result, result.title)}>{t("import.showReport")}</Button>
      {/if}
      {#if result.ok && result.playlistId}
        <Button
          variant="primary"
          onclick={() => {
            dialogs.close();
            ui.open({ name: "playlist", id: result.playlistId! });
          }}>{t("import.openPlaylist")}</Button>
      {:else}
        <Button variant="primary" onclick={() => dialogs.close()}>{t("import.close")}</Button>
      {/if}
    {:else}
      <Button variant="secondary" onclick={() => jobId && imports.cancel(jobId)}>{t("import.cancel")}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  .row {
    display: flex;
    gap: var(--s2);
  }
  .field {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 12px;
    border-radius: var(--r-md);
    background: var(--field);
    color: var(--text-3);
  }
  .field input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    font-size: var(--fs-sm);
    color: var(--text);
  }
  .services {
    margin: 8px 2px 0;
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  .error,
  .note {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 14px 0 0;
    font-size: var(--fs-sm);
    line-height: 1.45;
    color: var(--text-2);
  }
  .error {
    color: var(--text);
  }
  .error :global(svg),
  .note :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  .loading {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 18px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .preview {
    display: flex;
    gap: var(--s4);
    align-items: center;
    margin-top: 18px;
  }
  .meta {
    display: grid;
    gap: 3px;
    min-width: 0;
  }
  .kind {
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  h3 {
    margin: 0;
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  .owner,
  .count {
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .spacer {
    flex: 1;
  }
</style>
