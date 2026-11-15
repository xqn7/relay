<script lang="ts">
  import { fade } from "svelte/transition";
  import { menu } from "../stores/menu.svelte";
  import Icon from "./Icon.svelte";

  let el = $state<HTMLDivElement>();
  let pos = $state({ left: 0, top: 0 });

  // Keep the menu inside the window, flipping it left/up when it would overflow.
  $effect(() => {
    void menu.items;
    if (!menu.open || !el) return;
    const r = el.getBoundingClientRect();
    pos = {
      left: menu.x + r.width > innerWidth - 8 ? Math.max(8, menu.x - r.width) : menu.x,
      top: menu.y + r.height > innerHeight - 8 ? Math.max(8, menu.y - r.height) : menu.y,
    };
    el.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  });

  function onKey(e: KeyboardEvent) {
    if (!menu.open) return;
    if (e.key === "Escape") {
      e.preventDefault();
      menu.close();
    }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const buttons = [...(el?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
      const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
      buttons[(i + (e.key === "ArrowDown" ? 1 : -1) + buttons.length) % buttons.length]?.focus();
    }
  }
</script>

<svelte:window onkeydown={onKey} onblur={() => menu.close()} onresize={() => menu.close()} />

{#if menu.open}
  <div class="catcher" role="presentation" onpointerdown={() => menu.close()} oncontextmenu={(e) => (e.preventDefault(), menu.close())}></div>
  <div class="menu glass" role="menu" bind:this={el} style:left="{pos.left}px" style:top="{pos.top}px" transition:fade={{ duration: 110 }}>
    {#each menu.items as item, i (i + item.label)}
      {#if item.divider}<div class="divider" role="separator"></div>{/if}
      <button
        role="menuitem"
        disabled={item.disabled}
        onclick={() => {
          if (!item.submenu) menu.close();
          item.action();
        }}>
        <Icon name={item.icon} size={16} />
        <span class="truncate">{item.label}</span>
        {#if item.submenu}<span class="more"><Icon name="forward" size={14} /></span>{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .catcher {
    position: fixed;
    inset: 0;
    z-index: 90;
  }
  .divider {
    height: 1px;
    margin: 4px 6px;
    background: var(--hairline);
  }
  .more {
    margin-left: auto;
    display: grid;
  }
  .menu {
    position: fixed;
    z-index: 91;
    min-width: 200px;
    max-width: 300px;
    max-height: calc(100vh - 16px);
    overflow-y: auto;
    padding: 5px;
    border-radius: var(--r-md);
    background: rgba(var(--panel), 0.78);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.35);
  }
  button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    font-size: var(--fs-sm);
    text-align: left;
    color: var(--text);
  }
  button :global(svg) {
    color: var(--text-2);
  }
  button:hover:not(:disabled),
  button:focus-visible {
    background: var(--hover);
    outline: none;
  }
  button:disabled {
    opacity: 0.4;
  }
</style>
