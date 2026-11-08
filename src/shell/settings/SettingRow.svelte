<script lang="ts">
  import type { Snippet } from "svelte";

  let { label, desc, children }: { label: string; desc?: string; children: Snippet } = $props();
</script>

<div class="row">
  <div class="text">
    <div class="label">{label}</div>
    {#if desc}<div class="desc">{desc}</div>{/if}
  </div>
  <div class="control">{@render children()}</div>
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
    border-top: 1px solid rgba(255, 255, 255, 0.04);
  }
  .text {
    min-width: 0;
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
