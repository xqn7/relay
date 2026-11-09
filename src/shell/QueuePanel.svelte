<script lang="ts">
  import { flip } from "svelte/animate";
  import { fly } from "svelte/transition";
  import { artistLine } from "../lib/api";
  import Button from "../lib/components/Button.svelte";
  import Cover from "../lib/components/Cover.svelte";
  import Icon from "../lib/components/Icon.svelte";
  import IconButton from "../lib/components/IconButton.svelte";
  import SourceBadge from "../lib/components/SourceBadge.svelte";
  import { t } from "../lib/i18n/index.svelte";
  import { player } from "../lib/stores/player.svelte";
  import { ui } from "../lib/stores/ui.svelte";
  import { dialogs } from "../lib/stores/dialogs.svelte";
  import { library } from "../lib/stores/library.svelte";

  // A soft spring: overshoots slightly, then settles.
  const spring = (x: number) => {
    const s = 0.9;
    const t = x - 1;
    return t * t * ((s + 1) * t + s) + 1;
  };

  let drag = $state<{ uid: string; startY: number; offset: number; rowH: number } | null>(null);
  const base = $derived(player.queue.index + 1);

  function startDrag(e: PointerEvent, uid: string) {
    if (e.button !== 0) return;
    e.preventDefault();
    const row = (e.currentTarget as HTMLElement).closest<HTMLElement>(".item")!;
    drag = { uid, startY: e.clientY, offset: 0, rowH: row.getBoundingClientRect().height + 2 };
  }

  function moveDrag(e: PointerEvent) {
    if (!drag) return;
    const from = player.queue.items.findIndex((i) => i.uid === drag!.uid);
    let offset = e.clientY - drag.startY;
    const steps = Math.round(offset / drag.rowH);
    const to = Math.min(player.queue.items.length - 1, Math.max(base, from + steps));
    if (to !== from) {
      player.move(from, to);
      // The row moved in the DOM; shift the origin so it stays under the pointer.
      drag.startY += (to - from) * drag.rowH;
      offset = e.clientY - drag.startY;
    }
    drag.offset = offset;
  }

  function endDrag() {
    drag = null;
  }

  function saveQueue() {
    const tracks = player.queue.items.map((i) => i.track);
    dialogs.open({
      kind: "prompt",
      title: t("queue.save"),
      label: t("playlist.name"),
      value: t("queue.defaultName"),
      confirm: t("playlist.create"),
      onConfirm: async (name) => {
        const id = await library.createPlaylist(name, tracks);
        if (id) ui.toast(t("toast.addedToPlaylist", { name, n: tracks.length }), "check", { label: t("toast.open"), run: () => ui.open({ name: "playlist", id }) });
      },
    });
  }
</script>

<!-- Tracked on the window: reordering moves the row in the DOM, which would drop pointer capture. -->
<svelte:window onpointermove={moveDrag} onpointerup={endDrag} onpointercancel={endDrag} />

<aside class="panel glass" transition:fly={{ x: 24, duration: 180 }}>
  <header>
    <h2>{t("queue.title")}</h2>
    <IconButton icon="close" label={t("win.close")} onclick={() => (ui.queueOpen = false)} />
  </header>

  <div class="scroll">
    {#if player.current}
      <h3>{t("queue.nowPlaying")}</h3>
      <div class="item now">
        <Cover src={player.current.track.cover} size={40} />
        <div class="text">
          <div class="title truncate">{player.current.track.title}</div>
          <div class="artist"><SourceBadge source={player.current.track.source} /><span class="truncate">{artistLine(player.current.track)}</span></div>
        </div>
      </div>
    {/if}

    <h3>{t("queue.nextUp")}</h3>
    {#if player.upcoming.length === 0}
      <p class="empty">{t("queue.empty")}</p>
    {:else}
      <div class="list" class:dragging={!!drag}>
        {#each player.upcoming as item (item.uid)}
          <div
            class="item"
            class:lifted={drag?.uid === item.uid}
            style:transform={drag?.uid === item.uid ? `translateY(${drag.offset}px)` : undefined}
            animate:flip={{ duration: drag?.uid === item.uid ? 0 : 320, easing: spring }}
            role="button"
            tabindex="0"
            ondblclick={() => player.jumpTo(item.uid)}
            onkeydown={(e) => e.key === "Enter" && player.jumpTo(item.uid)}>
            <span class="grip" role="button" tabindex="-1" aria-label={t("queue.drag")} title={t("queue.drag")} onpointerdown={(e) => startDrag(e, item.uid)}>
              <Icon name="grip" size={16} />
            </span>
            <Cover src={item.track.cover} size={40} />
            <div class="text">
              <div class="title truncate">{item.track.title}</div>
              <div class="artist"><SourceBadge source={item.track.source} /><span class="truncate">{artistLine(item.track)}</span></div>
            </div>
            <button class="remove" aria-label={t("queue.remove")} onclick={() => player.remove(item.uid)}><Icon name="close" size={15} /></button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <footer>
    <Button variant="secondary" icon="trash" disabled={player.upcoming.length === 0} onclick={() => player.clearUpcoming()}>{t("queue.clear")}</Button>
    <Button variant="ghost" icon="plus" disabled={!player.current} onclick={saveQueue}>{t("queue.save")}</Button>
  </footer>
</aside>

<style>
  .panel {
    position: fixed;
    top: 56px;
    right: 10px;
    bottom: calc(var(--player-h) + 20px);
    width: 340px;
    z-index: 35;
    display: flex;
    flex-direction: column;
    border-radius: var(--r-lg);
    background: rgba(18, 18, 20, 0.7);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 10px 6px 18px;
  }
  h2 {
    margin: 0;
    font-size: var(--fs-lg);
    font-weight: 700;
  }
  h3 {
    margin: 14px 8px 6px;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-3);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .scroll {
    flex: 1;
    overflow-y: auto;
    padding: 0 8px 8px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .item {
    position: relative;
    display: grid;
    grid-template-columns: auto auto 1fr auto;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 2px;
    border-radius: var(--r-md);
    background: transparent;
    outline: none;
    transition: background var(--t-fast) var(--ease), box-shadow var(--t) var(--ease);
  }
  .item.now {
    grid-template-columns: auto 1fr;
    padding-left: 8px;
    background: var(--hover);
  }
  .item:hover,
  .item:focus-visible {
    background: var(--hover);
  }
  .lifted {
    z-index: 2;
    background: rgba(40, 40, 44, 0.95) !important;
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.35);
  }
  .dragging .item:not(.lifted) {
    pointer-events: none;
  }
  .text {
    min-width: 0;
  }
  .title {
    font-size: var(--fs-sm);
    font-weight: 500;
  }
  .artist {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-top: 2px;
    font-size: var(--fs-xs);
    color: var(--text-2);
    min-width: 0;
  }
  .grip {
    display: grid;
    place-items: center;
    width: 22px;
    height: 32px;
    color: var(--text-3);
    cursor: grab;
    touch-action: none;
  }
  .lifted .grip {
    cursor: grabbing;
  }
  .remove {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-3);
    opacity: 0;
  }
  .item:hover .remove,
  .item:focus-within .remove {
    opacity: 1;
  }
  .remove:hover {
    color: var(--text);
    background: var(--hover);
  }
  .empty {
    margin: 6px 8px;
    font-size: var(--fs-sm);
    color: var(--text-3);
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s2);
    padding: 10px 12px 12px;
    border-top: 1px solid var(--hairline);
  }
</style>
