<script lang="ts">
  import { onDestroy } from "svelte";
  import { api } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import Segmented from "../../lib/components/Segmented.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { imports, openReport } from "../../lib/stores/imports.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import ImportProgress from "./ImportProgress.svelte";
  import { errorText } from "./importText";

  let method = $state<"links" | "export">("links");
  let text = $state("");
  let error = $state("");
  let busy = $state(false);
  let jobId = $state<string | null>(null);
  const job = $derived(imports.get(jobId));
  const result = $derived(job?.result ?? null);
  const linkCount = $derived((text.match(/open\.spotify\.com\/(intl-[a-z-]+\/)?track\/|spotify:track:/g) ?? []).length);

  onDestroy(() => {
    if (imports.watched === jobId) imports.watched = null;
  });

  function begin(id: string) {
    imports.add(id, t("liked.title"));
    imports.watched = id;
    jobId = id;
  }

  async function fromLinks() {
    error = "";
    busy = true;
    try {
      begin(await api.importLikedLinks(text));
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function fromFile() {
    error = "";
    busy = true;
    try {
      const id = await api.importLikedFile();
      if (id) begin(id);
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  function close() {
    if (jobId && !result) ui.toast(t("import.continues"), "download");
    dialogs.close();
  }
</script>

<Dialog title={t("importLiked.title")} sub={t("importLiked.sub")} width={540} onclose={close}>
  {#if !jobId}
    <Segmented
      label={t("importLiked.method")}
      value={method}
      options={[
        { value: "links", label: t("importLiked.links") },
        { value: "export", label: t("importLiked.export") },
      ]}
      onchange={(v) => ((method = v), (error = ""))} />

    {#if method === "links"}
      <ol>
        <li>{t("importLiked.links1")}</li>
        <li>{t("importLiked.links2")}</li>
        <li>{t("importLiked.links3")}</li>
      </ol>
      <textarea bind:value={text} placeholder={t("importLiked.pastePlaceholder")} spellcheck="false"></textarea>
      <div class="count">{linkCount > 0 ? t("importLiked.found", { n: linkCount }) : ""}</div>
    {:else}
      <ol>
        <li>{t("importLiked.export1")}</li>
        <li>{t("importLiked.export2")}</li>
        <li>{t("importLiked.export3")}</li>
      </ol>
      <p class="hint">{t("importLiked.exportHint")}</p>
    {/if}
    {#if error}<p class="error"><Icon name="alert" size={15} />{error}</p>{/if}
  {:else if job}
    <ImportProgress {job} />
  {/if}

  {#snippet footer()}
    {#if !jobId}
      {#if method === "links"}
        <Button variant="primary" icon="download" busy={busy} disabled={linkCount === 0} onclick={fromLinks}>{t("importLiked.importLinks")}</Button>
      {:else}
        <Button variant="primary" icon="upload" busy={busy} onclick={fromFile}>{t("importLiked.chooseFile")}</Button>
      {/if}
    {:else if result}
      {#if result.ok && result.unmatched.length + result.skipped.length > 0}
        <Button variant="secondary" onclick={() => openReport(result, t("liked.title"))}>{t("import.showReport")}</Button>
      {/if}
      <Button
        variant="primary"
        onclick={() => {
          dialogs.close();
          if (result.ok) ui.open({ name: "liked" });
        }}>{result.ok ? t("importLiked.open") : t("import.close")}</Button>
    {:else}
      <Button variant="secondary" onclick={() => jobId && imports.cancel(jobId)}>{t("import.cancel")}</Button>
    {/if}
  {/snippet}
</Dialog>

<style>
  ol {
    margin: 16px 0 12px;
    padding-left: 20px;
    font-size: var(--fs-sm);
    line-height: 1.65;
    color: var(--text-2);
  }
  textarea {
    width: 100%;
    height: 130px;
    padding: 10px 12px;
    resize: none;
    border: 0;
    border-radius: var(--r-md);
    outline: none;
    background: var(--field);
    font: inherit;
    font-size: var(--fs-xs);
    color: var(--text);
  }
  .count {
    height: 18px;
    margin-top: 6px;
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  .hint {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-3);
    line-height: 1.5;
  }
  .error {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 10px 0 0;
    font-size: var(--fs-sm);
  }
</style>
