<script lang="ts">
  import logo from "../lib/assets/logo.svg";
  import Icon from "../lib/components/Icon.svelte";
  import type { IconName } from "../lib/icons";
  import { t } from "../lib/i18n/index.svelte";
  import { ui, type Route } from "../lib/stores/ui.svelte";

  let { scrollTop }: { scrollTop: () => number } = $props();

  const items: { route: Route["name"]; icon: IconName; label: () => string }[] = [
    { route: "home", icon: "home", label: () => t("nav.home") },
    { route: "library", icon: "library", label: () => t("nav.library") },
  ];
</script>

<nav class="sidebar">
  <div class="logo" data-tauri-drag-region><img src={logo} alt="Relay" width="30" height="30" draggable="false" /></div>
  <div class="rail">
    <button class:active={ui.route.name === "home"} aria-label={items[0].label()} title={items[0].label()} onclick={() => ui.go({ name: "home" }, scrollTop())}>
      <Icon name="home" size={20} />
    </button>
    <button class:active={ui.searchOpen} aria-label={t("nav.search")} title={t("nav.search")} onclick={() => (ui.searchOpen = true)}>
      <Icon name="search" size={20} />
    </button>
    <button class:active={ui.route.name === "library"} aria-label={items[1].label()} title={items[1].label()} onclick={() => ui.go({ name: "library" }, scrollTop())}>
      <Icon name="library" size={20} />
    </button>
  </div>
  <button class="settings" class:active={ui.settingsOpen} aria-label={t("nav.settings")} title={t("nav.settings")} onclick={() => (ui.settingsOpen = true)}>
    <Icon name="settings" size={20} />
  </button>
</nav>

<style>
  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
    width: calc(var(--sidebar-w) + 12px);
    z-index: 20;
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 12px 0 14px 6px;
  }
  .logo {
    display: grid;
    place-items: center;
    height: 40px;
    margin-bottom: 14px;
  }
  .logo img {
    pointer-events: none;
  }
  .rail {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px;
    border-radius: var(--r-lg);
    background: rgba(0, 0, 0, 0.22);
  }
  button {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: var(--r-md);
    background: transparent;
    color: var(--text-3);
    transition: color var(--t-fast) var(--ease), background var(--t-fast) var(--ease);
  }
  button:hover {
    color: var(--text);
  }
  button.active {
    color: var(--text);
    background: var(--hover);
  }
  .settings {
    margin-top: auto;
    margin-bottom: 14px;
  }
</style>
