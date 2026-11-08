<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import Icon from "../lib/components/Icon.svelte";
  import { t } from "../lib/i18n/index.svelte";

  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    const sync = () => win.isMaximized().then((m) => (maximized = m));
    sync();
    const off = win.onResized(sync);
    return () => {
      off.then((f) => f());
    };
  });
</script>

<div class="controls">
  <button aria-label={t("win.minimize")} onclick={() => win.minimize()}><Icon name="minimize" size={15} /></button>
  <button aria-label={maximized ? t("win.restore") : t("win.maximize")} onclick={() => win.toggleMaximize()}>
    <Icon name={maximized ? "restore" : "maximize"} size={13} />
  </button>
  <button class="close" aria-label={t("win.close")} onclick={() => win.close()}><Icon name="close" size={15} /></button>
</div>

<style>
  .controls {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: var(--r-md);
    background: rgba(0, 0, 0, 0.28);
  }
  button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 26px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
  }
  button:hover {
    background: var(--hover);
    color: var(--text);
  }
  .close:hover {
    background: rgba(255, 255, 255, 0.16);
  }
</style>
