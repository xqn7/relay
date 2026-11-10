<!-- Name prompts and confirmations: short dialogs with one field or one question. -->
<script lang="ts">
  import { untrack } from "svelte";
  import Button from "../../lib/components/Button.svelte";
  import Dialog from "../../lib/components/Dialog.svelte";
  import Field from "../../lib/components/Field.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { dialogs, type Dialog as D } from "../../lib/stores/dialogs.svelte";

  let { d }: { d: Extract<D, { kind: "prompt" } | { kind: "confirm" }> } = $props();
  let value = $state(untrack(() => (d.kind === "prompt" ? d.value : "")));

  function confirm() {
    // Closing clears the dialog `d` comes from, so keep this one before closing.
    const dialog = d;
    if (dialog.kind === "prompt") {
      const v = value.trim();
      if (!v) return;
      dialogs.close();
      dialog.onConfirm(v);
    } else {
      dialogs.close();
      dialog.onConfirm();
    }
  }
</script>

<Dialog title={d.title} width={400} onclose={() => dialogs.close()}>
  {#if d.kind === "prompt"}
    <form
      onsubmit={(e) => {
        e.preventDefault();
        confirm();
      }}>
      <Field label={d.label} bind:value autofocus />
    </form>
  {:else}
    <p>{d.body}</p>
  {/if}
  {#snippet footer()}
    <Button variant="ghost" onclick={() => dialogs.close()}>{t("dialog.cancel")}</Button>
    <Button variant="primary" disabled={d.kind === "prompt" && !value.trim()} onclick={confirm}>{d.confirm}</Button>
  {/snippet}
</Dialog>

<style>
  p {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
    line-height: 1.5;
  }
</style>
