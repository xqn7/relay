<!-- A titled section with one row of tiles that scrolls sideways (Home, Discover). -->
<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    sub = "",
    action,
    rows = 1,
    children,
  }: {
    title: string;
    sub?: string;
    action?: { label: string; run: () => void };
    /** Rows of tiles; more than one fills column by column. */
    rows?: number;
    children: Snippet;
  } = $props();
</script>

<section class="shelf">
  <div class="head">
    <div>
      <h2>{title}</h2>
      {#if sub}<p>{sub}</p>{/if}
    </div>
    {#if action}<button class="action" onclick={action.run}>{action.label}</button>{/if}
  </div>
  <div class="row stagger" style:grid-template-rows="repeat({rows}, auto)">{@render children()}</div>
</section>

<style>
  .shelf {
    margin-bottom: var(--s6);
  }
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--s4);
    margin-bottom: var(--s2);
  }
  h2 {
    margin: 0;
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  p {
    margin: 2px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-2);
  }
  .action {
    flex: none;
    padding: 0;
    border: 0;
    background: none;
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--text-2);
  }
  .action:hover {
    color: var(--text);
  }
  .row {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 168px;
    gap: 4px;
    margin: 0 -8px;
    padding-bottom: 6px;
    overflow-x: auto;
    scroll-snap-type: x proximity;
  }
  .row > :global(*) {
    scroll-snap-align: start;
  }
</style>
