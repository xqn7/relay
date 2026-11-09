<script lang="ts">
  import type { IconName } from "../icons";
  import Icon from "./Icon.svelte";

  let {
    icon,
    label,
    size = 18,
    active = false,
    disabled = false,
    variant = "ghost",
    onclick,
  }: {
    icon: IconName;
    label: string;
    size?: number;
    active?: boolean;
    disabled?: boolean;
    variant?: "ghost" | "solid" | "primary";
    onclick?: (e: MouseEvent) => void;
  } = $props();
</script>

<button type="button" class="icon-btn {variant}" class:active title={label} aria-label={label} aria-pressed={active || undefined} {disabled} {onclick}>
  <Icon name={icon} {size} />
</button>

<style>
  .icon-btn {
    display: inline-grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-2);
    transition: background var(--t-fast) var(--ease), color var(--t-fast) var(--ease), transform var(--t-fast) var(--ease);
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .icon-btn:active:not(:disabled) {
    transform: scale(0.94);
  }
  .icon-btn.active {
    color: var(--text);
    background: var(--selected);
  }
  .icon-btn.solid {
    background: var(--glass-strong);
    color: var(--text);
  }
  .icon-btn.primary {
    width: 36px;
    height: 36px;
    border-radius: var(--r-full);
    background: var(--primary);
    color: var(--on-primary);
  }
  .icon-btn.primary:hover:not(:disabled) {
    background: #fff;
    color: var(--on-primary);
  }
  .icon-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }
</style>
