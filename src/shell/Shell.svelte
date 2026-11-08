<script lang="ts">
  import { tick } from "svelte";
  import { fade, fly } from "svelte/transition";
  import Backdrop from "../lib/backdrop/Backdrop.svelte";
  import Menu from "../lib/components/Menu.svelte";
  import { player } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import Artist from "../pages/Artist.svelte";
  import Home from "../pages/Home.svelte";
  import Library from "../pages/Library.svelte";
  import PlayerBar from "./PlayerBar.svelte";
  import QueuePanel from "./QueuePanel.svelte";
  import SearchPopup from "./SearchPopup.svelte";
  import Settings from "./settings/Settings.svelte";
  import Sidebar from "./Sidebar.svelte";
  import TitleBar from "./TitleBar.svelte";

  let { onSignOut }: { onSignOut: () => void } = $props();

  let main = $state<HTMLElement>();
  const scrollTop = () => main?.scrollTop ?? 0;
  const backdropCover = $derived(player.standIn?.cover ?? player.current?.track.cover ?? null);

  // Restore the remembered scroll position after each page change.
  $effect(() => {
    const entry = ui.entry;
    tick().then(() => main?.scrollTo({ top: entry.scroll }));
  });

  function onKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
    if ((e.ctrlKey && e.key.toLowerCase() === "k") || (e.key === "/" && !typing)) {
      e.preventDefault();
      ui.searchOpen = true;
    } else if (e.code === "Space" && !typing && !(e.target instanceof HTMLButtonElement) && !ui.searchOpen) {
      e.preventDefault();
      player.toggle();
    } else if (e.altKey && e.key === "ArrowLeft") ui.back(scrollTop());
    else if (e.altKey && e.key === "ArrowRight") ui.forward(scrollTop());
  }

  function onMouse(e: MouseEvent) {
    // Mouse back/forward buttons.
    if (e.button === 3) ui.back(scrollTop());
    if (e.button === 4) ui.forward(scrollTop());
  }
</script>

<svelte:window onkeydown={onKey} onmouseup={onMouse} />

<Backdrop cover={backdropCover} />
<TitleBar {scrollTop} />
<Sidebar {scrollTop} />

<main bind:this={main} class:with-queue={ui.queueOpen}>
  <div class="pages">
  {#key ui.entry.key}
    <div class="page" in:fly={{ x: 14 * ui.direction, duration: 180, delay: 60 }} out:fade={{ duration: 80 }}>
      {#if ui.route.name === "home"}
        <Home />
      {:else if ui.route.name === "library"}
        <Library />
      {:else if ui.route.name === "artist"}
        <Artist name={ui.route.artistName} />
      {/if}
    </div>
  {/key}
  </div>
</main>

{#if ui.queueOpen}<QueuePanel />{/if}
<PlayerBar />
{#if ui.searchOpen}<SearchPopup />{/if}
{#if ui.settingsOpen}<Settings {onSignOut} />{/if}
<Menu />

<style>
  main {
    position: fixed;
    top: 0;
    left: calc(var(--sidebar-w) + 12px);
    right: 0;
    bottom: 0;
    z-index: 10;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 56px 24px calc(var(--player-h) + 40px) 18px;
    transition: right var(--t) var(--ease);
  }
  main.with-queue {
    right: 356px;
  }
  /* Old and new page share one grid cell, so the outgoing page fades in place. */
  .pages {
    display: grid;
  }
  .page {
    grid-area: 1 / 1;
    min-width: 0;
  }
</style>
