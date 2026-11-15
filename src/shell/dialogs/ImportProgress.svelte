<script lang="ts">
  import Icon from "../../lib/components/Icon.svelte";
  import Spinner from "../../lib/components/Spinner.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import type { Job } from "../../lib/stores/imports.svelte";
  import { errorText } from "./importText";

  let { job }: { job: Job } = $props();
  const result = $derived(job.result);
  const pct = $derived(job.total ? job.done / job.total : 0);
</script>

{#if !result}
  <div class="progress">
    <div class="line">
      <Spinner size={16} />
      <span>
        {#if job.phase === "starting"}
          {t("import.starting")}
        {:else}
          {t(job.phase === "reading" ? "import.reading" : "import.matching", { done: job.done, total: job.total })}
        {/if}
      </span>
    </div>
    <div class="bar"><div class="fill" style:width="{Math.round(pct * 100)}%"></div></div>
    <p class="hint">{t("import.matchingHint")}</p>
  </div>
{:else if result.cancelled}
  <p class="status"><Icon name="info" size={16} />{t("import.cancelled")}</p>
{:else if !result.ok}
  <p class="status"><Icon name="alert" size={16} />{errorText(result.error)}</p>
{:else}
  <div class="done">
    <p class="status"><Icon name="check" size={16} />{result.updated ? t("import.updated", { title: result.title || job.title }) : t("import.done", { title: result.title || job.title })}</p>
    <ul>
      {#if result.updated}
        <li>{t("import.changes", { added: result.added, removed: result.removed })}</li>
      {:else}
        <li>{t("import.added", { n: result.added || result.total })}</li>
      {/if}
      {#if result.unmatched.length}<li>{t("import.unmatched", { n: result.unmatched.length })}</li>{/if}
      {#if result.skipped.length}<li>{t("import.skipped", { n: result.skipped.length })}</li>{/if}
      {#if result.truncated}<li>{t("import.truncatedDone")}</li>{/if}
    </ul>
  </div>
{/if}

<style>
  .progress {
    display: grid;
    gap: 12px;
    padding: 6px 0 2px;
  }
  .line,
  .status {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    font-size: var(--fs-md);
  }
  .bar {
    height: 4px;
    border-radius: 2px;
    background: rgba(var(--ink), 0.1);
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: rgba(var(--ink), 0.8);
    transition: width var(--t) var(--ease);
  }
  .hint {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-3);
  }
  ul {
    margin: 10px 0 0;
    padding-left: 26px;
    font-size: var(--fs-sm);
    color: var(--text-2);
    line-height: 1.7;
  }
</style>
