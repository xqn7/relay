<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "../lib/components/Icon.svelte";
  import type { IconName } from "../lib/icons";

  let { icon, title, body, children }: { icon: IconName; title: string; body?: string; children?: Snippet } = $props();
</script>

<div class="empty">
  <div class="mark"><Icon name={icon} size={26} /></div>
  <h2>{title}</h2>
  {#if body}<p>{body}</p>{/if}
  {#if children}{@render children()}{/if}
</div>

<style>
  .empty {
    display: grid;
    justify-items: center;
    gap: 10px;
    max-width: 420px;
    margin: 18vh auto 0;
    text-align: center;
  }
  .mark {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    margin-bottom: 6px;
    border-radius: var(--r-lg);
    background: var(--glass);
    color: var(--text-2);
  }
  h2 {
    margin: 0;
    font-size: var(--fs-xl);
    font-weight: 700;
  }
  p {
    margin: 0 0 10px;
    color: var(--text-2);
    line-height: 1.5;
  }
</style>
