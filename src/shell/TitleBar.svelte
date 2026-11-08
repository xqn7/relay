<script lang="ts">
  import Icon from "../lib/components/Icon.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import WindowControls from "./WindowControls.svelte";

  let { scrollTop }: { scrollTop: () => number } = $props();
</script>

<div class="titlebar" data-tauri-drag-region>
  <div class="right">
    <div class="nav">
      <button aria-label={t("nav.back")} disabled={!ui.canBack} onclick={() => ui.back(scrollTop())}><Icon name="back" size={16} /></button>
      <button aria-label={t("nav.forward")} disabled={!ui.canForward} onclick={() => ui.forward(scrollTop())}><Icon name="forward" size={16} /></button>
    </div>
    <WindowControls />
  </div>
</div>

<style>
  .titlebar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    height: 52px;
    z-index: 30;
    display: flex;
    justify-content: flex-end;
    align-items: center;
    padding: 0 12px;
  }
  .right {
    display: flex;
    gap: var(--s2);
  }
  .nav {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: var(--r-md);
    background: rgba(0, 0, 0, 0.28);
  }
  .nav button {
    display: grid;
    place-items: center;
    width: 28px;
    height: 26px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
  }
  .nav button:hover:not(:disabled) {
    background: var(--hover);
    color: var(--text);
  }
  .nav button:disabled {
    opacity: 0.35;
    cursor: default;
  }
</style>
