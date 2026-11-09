<script lang="ts">
  import { artistLine, type Skipped, type Track } from "../../lib/api";
  import Button from "../../lib/components/Button.svelte";
  import Cover from "../../lib/components/Cover.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Icon from "../../lib/components/Icon.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs } from "../../lib/stores/dialogs.svelte";
  import { skippedReason } from "./importText";

  let { title, unmatched, skipped, truncated }: { title: string; unmatched: Track[]; skipped: Skipped[]; truncated: boolean } = $props();

  // Deleted videos come without titles; they are counted instead of listed.
  const named = $derived(skipped.filter((s) => s.title));
  const unnamed = $derived(skipped.length - named.length);
</script>

<Dialog title={t("report.title")} sub={title} width={560} onclose={() => dialogs.close()}>
  {#if truncated}
    <p class="note"><Icon name="info" size={15} />{t("import.truncatedDone")}</p>
  {/if}
  {#if unmatched.length}
    <h4>{t("report.unmatchedHead", { n: unmatched.length })}</h4>
    <p class="explain">{t("report.unmatchedExplain")}</p>
    {#each unmatched as track (track.id)}
      <div class="item">
        <Cover src={track.cover} size={36} />
        <div class="text">
          <div class="name truncate">{track.title}</div>
          <div class="sub truncate">{artistLine(track)}</div>
        </div>
        <Button variant="secondary" icon="replace" onclick={() => dialogs.open({ kind: "fixMatch", track })}>{t("report.fix")}</Button>
      </div>
    {/each}
  {/if}
  {#if skipped.length}
    <h4>{t("report.skippedHead", { n: skipped.length })}</h4>
    {#if unnamed > 0}<p class="explain">{t("report.deletedCount", { n: unnamed })}</p>{/if}
    {#each named as s, i (i)}
      <div class="item plain">
        <div class="text">
          <div class="name truncate">{s.title}</div>
          <div class="sub truncate">{s.artist ? `${s.artist} · ` : ""}{skippedReason(s)}</div>
        </div>
      </div>
    {/each}
  {/if}
  {#if !unmatched.length && !skipped.length}
    <p class="explain">{t("report.allGood")}</p>
  {/if}
</Dialog>

<style>
  h4 {
    margin: 16px 0 4px;
    font-size: var(--fs-sm);
    font-weight: 700;
  }
  h4:first-child {
    margin-top: 4px;
  }
  .explain {
    margin: 0 0 8px;
    font-size: var(--fs-xs);
    color: var(--text-3);
    line-height: 1.5;
  }
  .note {
    display: flex;
    gap: 8px;
    margin: 0 0 6px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 0;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .sub {
    font-size: var(--fs-xs);
    color: var(--text-2);
  }
</style>
