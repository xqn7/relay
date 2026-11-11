<!-- "Update downloaded" with its changelog, and the hardware behind the visual quality advice. -->
<script lang="ts">
  import Button from "../../lib/components/Button.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { desktop } from "../../lib/stores/desktop.svelte";
  import { dialogs, type Dialog as D } from "../../lib/stores/dialogs.svelte";
  import { ui } from "../../lib/stores/ui.svelte";

  let { d }: { d: Extract<D, { kind: "update" } | { kind: "hardware" }> } = $props();
  let busy = $state(false);

  async function install() {
    busy = true;
    try {
      await desktop.install();
    } catch {
      busy = false;
      ui.toast(t("update.installFailed"), "alert");
    }
  }
</script>

{#if d.kind === "update"}
  <Dialog title={t("update.title", { version: d.info.version })} sub={t("update.sub")} width={460} onclose={() => dialogs.close()}>
    {#if d.info.notes.trim()}
      <pre class="notes">{d.info.notes.trim()}</pre>
    {:else}
      <p class="muted">{t("update.noNotes")}</p>
    {/if}
    {#snippet footer()}
      <Button variant="ghost" onclick={() => dialogs.close()}>{t("update.later")}</Button>
      <Button variant="primary" {busy} onclick={install}>{t("update.restart")}</Button>
    {/snippet}
  </Dialog>
{:else}
  {@const h = d.info.hardware}
  <Dialog title={t("quality.detailsTitle")} width={420} onclose={() => dialogs.close()}>
    <dl>
      <dt>{t("quality.cpu")}</dt>
      <dd>{h.cpu || "–"} · {t("quality.threads", { n: h.cores })}</dd>
      <dt>{t("quality.ram")}</dt>
      <dd>{h.ramGb} GB</dd>
      <dt>{t("quality.gpu")}</dt>
      <dd>{h.gpu || "–"} · {h.gpuIntegrated ? t("quality.integrated") : t("quality.dedicated")}</dd>
      <dt>{t("quality.display")}</dt>
      <dd>{h.width} × {h.height} · {h.refreshHz} Hz</dd>
    </dl>
  </Dialog>
{/if}

<style>
  .notes {
    margin: 0;
    max-height: 260px;
    overflow-y: auto;
    white-space: pre-wrap;
    font: inherit;
    font-size: var(--fs-sm);
    line-height: 1.55;
    color: var(--text-2);
  }
  .muted {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--text-3);
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 10px 18px;
    margin: 0;
    font-size: var(--fs-sm);
  }
  dt {
    color: var(--text-3);
  }
  dd {
    margin: 0;
    color: var(--text);
  }
</style>
