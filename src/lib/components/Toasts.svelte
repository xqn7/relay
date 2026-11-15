<script lang="ts">
  import { fly } from "svelte/transition";
  import { flip } from "svelte/animate";
  import { ui } from "../stores/ui.svelte";
  import Icon from "./Icon.svelte";
</script>

<div class="toasts" aria-live="polite">
  {#each ui.toasts as toast (toast.id)}
    <div class="toast glass" animate:flip={{ duration: 180 }} in:fly={{ y: 10, duration: 180 }} out:fly={{ y: 6, duration: 140 }}>
      {#if toast.icon}<Icon name={toast.icon} size={16} />{/if}
      <span>{toast.text}</span>
      {#if toast.action}
        <button
          class="action"
          onclick={() => {
            toast.action?.run();
            ui.dismiss(toast.id);
          }}>{toast.action.label}</button>
      {/if}
      {#if toast.progress != null}
        <div class="bar"><div class="fill" style:width="{Math.round(toast.progress * 100)}%"></div></div>
      {/if}
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    left: 50%;
    bottom: calc(var(--player-h) + 28px);
    transform: translateX(-50%);
    z-index: 80;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--s2);
    pointer-events: none;
  }
  .toast {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 520px;
    padding: 10px 16px;
    overflow: hidden;
    border-radius: var(--r-md);
    background: rgba(var(--panel), 0.72);
    font-size: var(--fs-sm);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  }
  .action {
    margin-left: 6px;
    padding: 4px 10px;
    border: 0;
    border-radius: var(--r-sm);
    background: var(--glass-raised);
    font-size: var(--fs-xs);
    font-weight: 700;
    color: var(--text);
    pointer-events: auto;
  }
  .action:hover {
    background: rgba(var(--ink), 0.2);
  }
  .toast :global(svg) {
    color: var(--text-2);
  }
  .bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    background: rgba(var(--ink), 0.1);
  }
  .fill {
    height: 100%;
    background: rgba(var(--ink), 0.7);
    transition: width var(--t) var(--ease);
  }
</style>
