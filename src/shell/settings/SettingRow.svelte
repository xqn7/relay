<script lang="ts">
  import type { Snippet } from "svelte";

  let { label, desc, children }: { label: string; desc?: string; children?: Snippet } = $props();

  let control = $state<HTMLElement>();
  // A row whose only control is a switch flips it when its text is clicked, like a label.
  function flip() {
    const only = control?.children.length === 1 ? control.children[0] : null;
    if (only instanceof HTMLButtonElement && only.getAttribute("role") === "switch") only.click();
  }
</script>

<div class="row">
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="text" onclick={flip}>
    <div class="label">{label}</div>
    {#if desc}<div class="desc">{desc}</div>{/if}
  </div>
  {#if children}<div class="control" bind:this={control}>{@render children()}</div>{/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--s5);
    padding: 12px 16px;
  }
  .row + :global(.row) {
    border-top: 1px solid rgba(var(--ink), 0.04);
  }
  .text {
    min-width: 0;
  }
  .row:has(.control > :global([role="switch"]:only-child)) .text {
    cursor: pointer;
  }
  .label {
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .desc {
    margin-top: 3px;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .control {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--s3);
  }
</style>
