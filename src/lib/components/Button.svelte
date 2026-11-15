<script lang="ts">
  import type { Snippet } from "svelte";
  import type { IconName } from "../icons";
  import Icon from "./Icon.svelte";

  let {
    variant = "secondary",
    icon,
    full = false,
    small = false,
    disabled = false,
    busy = false,
    type = "button",
    title,
    onclick,
    children,
  }: {
    variant?: "primary" | "secondary" | "ghost";
    icon?: IconName;
    full?: boolean;
    /** Compact, for toolbars. */
    small?: boolean;
    disabled?: boolean;
    busy?: boolean;
    type?: "button" | "submit";
    title?: string;
    onclick?: (e: MouseEvent) => void;
    children: Snippet;
  } = $props();
</script>

<button class="btn {variant}" class:full class:small {type} {title} disabled={disabled || busy} aria-busy={busy} {onclick}>
  {#if busy}
    <span class="spin"><Icon name="loader" size={16} /></span>
  {:else if icon}
    <Icon name={icon} size={16} />
  {/if}
  {@render children()}
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--s2);
    height: 34px;
    padding: 0 var(--s4);
    border: 0;
    border-radius: var(--r-md);
    font-size: var(--fs-sm);
    font-weight: 600;
    white-space: nowrap;
    transition: background var(--t-fast) var(--ease), opacity var(--t-fast) var(--ease), transform var(--t-fast) var(--ease);
  }
  .btn:active:not(:disabled) {
    transform: scale(0.98);
  }
  .full {
    width: 100%;
    height: 42px;
  }
  .small {
    height: 28px;
    padding: 0 var(--s3);
    border-radius: var(--r-sm);
  }
  .primary {
    background: var(--primary);
    color: var(--on-primary);
  }
  .primary:hover:not(:disabled) {
    background: var(--solid);
  }
  .secondary {
    background: var(--glass-strong);
    color: var(--text);
  }
  .secondary:hover:not(:disabled) {
    background: var(--glass-raised);
  }
  .ghost {
    background: transparent;
    color: var(--text-2);
  }
  .ghost:hover:not(:disabled) {
    color: var(--text);
    background: var(--hover);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .spin {
    display: grid;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
