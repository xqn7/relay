<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick, untrack } from "svelte";
  import { fade, fly } from "svelte/transition";
  import Backdrop from "../lib/backdrop/Backdrop.svelte";
  import Menu from "../lib/components/Menu.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { dnd } from "../lib/stores/dnd.svelte";
  import { player } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import AlbumPage from "../pages/AlbumPage.svelte";
  import ArtistPage from "../pages/ArtistPage.svelte";
  import FolderPage from "../pages/FolderPage.svelte";
  import Home from "../pages/Home.svelte";
  import Library from "../pages/Library.svelte";
  import ListPage from "../pages/ListPage.svelte";
  import TrashPage from "../pages/TrashPage.svelte";
  import { lyrics } from "../lib/stores/lyrics.svelte";
  import { offline } from "../lib/stores/offline.svelte";
  import DialogHost from "./dialogs/DialogHost.svelte";
  import FullscreenPlayer from "./fullscreen/FullscreenPlayer.svelte";
  import IdleScreen from "./idle/IdleScreen.svelte";
  import PlayerBar from "./PlayerBar.svelte";
  import QueuePanel from "./QueuePanel.svelte";
  import SearchPopup from "./SearchPopup.svelte";
  import Settings from "./settings/Settings.svelte";
  import Sidebar from "./Sidebar.svelte";
  import TitleBar from "./TitleBar.svelte";
  import UpNext from "./UpNext.svelte";

  let { onSignOut }: { onSignOut: () => void } = $props();

  let main = $state<HTMLElement>();
  const scrollTop = () => main?.scrollTop ?? 0;
  ui.scrollTop = scrollTop;
  // Artist and album pages tint the backdrop with their own art while they're open.
  const backdropCover = $derived(ui.heroCover ?? player.current?.track.cover ?? player.via?.cover ?? null);

  onMount(() => {
    let addToast: number | null = null;
    const offDrag = listen<string>("file-drag", ({ payload }) => {
      ui.filesOver = payload === "enter";
    });
    const offAdd = listen<{ done: number; total: number; finished: boolean; failed: string[]; converting: boolean }>("local-progress", ({ payload: p }) => {
      if (p.total === 0) {
        if (p.finished) ui.toast(t("local.nothingToAdd"), "info");
        return;
      }
      if (p.finished) {
        const added = p.total - p.failed.length;
        const text = p.failed.length ? t("local.addedSome", { n: added, failed: p.failed.length }) : t("local.added", { n: added });
        addToast = ui.progressToast(addToast, text, null, { label: t("local.show"), run: () => ui.open({ name: "local" }) });
        addToast = null;
      } else {
        const text = p.converting ? t("local.converting") : t("local.adding", { done: p.done, total: p.total });
        addToast = ui.progressToast(addToast, text, p.done / p.total);
      }
    });
    return () => {
      offDrag.then((f) => f());
      offAdd.then((f) => f());
    };
  });

  // Lyrics follow the current song (and load once the connection is back).
  $effect(() => {
    void offline.online;
    const track = player.current?.track ?? null;
    untrack(() => lyrics.follow(track));
  });

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
<Sidebar />

<main bind:this={main} class:with-queue={ui.queueOpen}>
  <div class="pages">
  {#key ui.entry.key}
    <div class="page" in:fly={{ x: 14 * ui.direction, duration: 180, delay: 60 }} out:fade={{ duration: 80 }}>
      {#if ui.route.name === "home"}
        <Home />
      {:else if ui.route.name === "library"}
        <Library tab={ui.route.tab} />
      {:else if ui.route.name === "playlist"}
        <ListPage mode="playlist" id={ui.route.id} />
      {:else if ui.route.name === "liked"}
        <ListPage mode="liked" />
      {:else if ui.route.name === "local"}
        <ListPage mode="local" />
      {:else if ui.route.name === "offline"}
        <ListPage mode="offline" />
      {:else if ui.route.name === "folder"}
        <FolderPage id={ui.route.id} />
      {:else if ui.route.name === "trash"}
        <TrashPage />
      {:else if ui.route.name === "album"}
        <AlbumPage source={ui.route.source} id={ui.route.id} />
      {:else if ui.route.name === "artist"}
        <ArtistPage source={ui.route.source} id={ui.route.id} name={ui.route.artistName} />
      {/if}
    </div>
  {/key}
  </div>
</main>

{#if ui.queueOpen}<QueuePanel />{/if}
<PlayerBar />
<UpNext />
{#if ui.fullscreenOpen}<FullscreenPlayer />{/if}
{#if ui.searchOpen}<SearchPopup />{/if}
{#if ui.settingsOpen}<Settings {onSignOut} />{/if}
<Menu />
<DialogHost />
<IdleScreen />
{#if dnd.payload}
  <div class="ghost glass" style:left="{dnd.x + 14}px" style:top="{dnd.y + 10}px">{dnd.payload.label}</div>
{/if}

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
  .ghost {
    position: fixed;
    z-index: 95;
    max-width: 260px;
    padding: 6px 12px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    border-radius: var(--r-full);
    background: rgba(24, 24, 27, 0.8);
    font-size: var(--fs-sm);
    font-weight: 600;
    pointer-events: none;
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
