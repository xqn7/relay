<script lang="ts">
  import { flip } from "svelte/animate";
  import { api, type Track } from "../api";
  import { t } from "../i18n/index.svelte";
  import { dnd, dropTarget, type Payload } from "../stores/dnd.svelte";
  import { fail, library } from "../stores/library.svelte";
  import { player } from "../stores/player.svelte";
  import { settings } from "../stores/settings.svelte";
  import { openTrackMenu } from "../trackMenu";
  import { insertIndex, visibleRange } from "../virtual";
  import TrackRow from "./TrackRow.svelte";

  /** Longer lists render only the rows near the screen. */
  const VIRTUAL_FROM = 150;
  const OVERSCAN = 12;

  let {
    tracks,
    itemIds,
    playlistId,
    reorderable = false,
    showAlbum = false,
    compact = false,
    onchanged,
  }: {
    tracks: Track[];
    /** Playlist row ids, parallel to `tracks`; enables remove and reorder. */
    itemIds?: string[];
    playlistId?: string;
    reorderable?: boolean;
    showAlbum?: boolean;
    compact?: boolean;
    /** Called after songs were removed or moved, so the page reloads its list. */
    onchanged?: () => void;
  } = $props();

  // Keys stay unique even when a list holds the same song twice.
  const keys = $derived(itemIds ?? tracks.map((x, i) => `${x.id}#${i}`));
  let selected = $state<Set<number>>(new Set());
  let anchor = 0;
  let listEl = $state<HTMLElement>();
  const rowHeight = $derived(compact ? 50 : 58);
  const virtual = $derived(tracks.length > VIRTUAL_FROM);
  let range = $state({ start: 0, end: VIRTUAL_FROM });
  const indices = $derived.by(() => {
    const [start, end] = virtual ? [range.start, range.end] : [0, tracks.length];
    return Array.from({ length: Math.max(0, end - start) }, (_, k) => start + k);
  });

  function scrollParent(el: HTMLElement): HTMLElement | null {
    for (let p = el.parentElement; p; p = p.parentElement) {
      const o = getComputedStyle(p).overflowY;
      if (o === "auto" || o === "scroll") return p;
    }
    return null;
  }

  // Scroll and resize events only; nothing runs while the list sits still.
  $effect(() => {
    if (!virtual || !listEl) return;
    const el = listEl;
    const scroller = scrollParent(el);
    const count = tracks.length;
    const h = rowHeight;
    const update = () => {
      const top = el.getBoundingClientRect().top - (scroller?.getBoundingClientRect().top ?? 0);
      const next = visibleRange(top, scroller?.clientHeight ?? window.innerHeight, h, count, OVERSCAN);
      if (next.start !== range.start || next.end !== range.end) range = next;
    };
    update();
    const target = scroller ?? window;
    target.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", update);
    return () => {
      target.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  });

  $effect(() => {
    void tracks;
    selected = new Set();
  });

  function select(e: MouseEvent, i: number) {
    if (e.shiftKey) {
      const [a, b] = [Math.min(anchor, i), Math.max(anchor, i)];
      selected = new Set(Array.from({ length: b - a + 1 }, (_, k) => a + k));
    } else if (e.ctrlKey || e.metaKey) {
      const next = new Set(selected);
      if (next.has(i)) next.delete(i);
      else next.add(i);
      selected = next;
      anchor = i;
    } else {
      selected = new Set([i]);
      anchor = i;
    }
  }

  function activate(i: number) {
    if (settings.values.doubleClick === "queue") player.activate(tracks, i);
    else player.playTracks(tracks, i, playlistId);
  }

  function selection(i: number): number[] {
    return selected.has(i) && selected.size > 1 ? [...selected].sort((a, b) => a - b) : [i];
  }

  function menuFor(e: MouseEvent, i: number) {
    if (!selected.has(i)) {
      selected = new Set([i]);
      anchor = i;
    }
    const picked = selection(i);
    openTrackMenu(e, {
      tracks,
      index: i,
      selection: picked.map((k) => tracks[k]),
      playlist: playlistId && itemIds ? { id: playlistId, itemIds: picked.map((k) => itemIds[k]), onRemoved: () => onchanged?.() } : undefined,
    });
  }

  function press(e: PointerEvent, i: number) {
    if ((e.target as HTMLElement).closest("button")) return;
    dnd.arm(e, () => {
      const picked = selection(i);
      const list = picked.map((k) => tracks[k]);
      return {
        kind: "tracks",
        tracks: list,
        label: list.length === 1 ? list[0].title : t("drag.songs", { n: list.length }),
        fromPlaylist: playlistId,
        itemIds: itemIds ? picked.map((k) => itemIds[k]) : undefined,
      };
    });
  }

  // ---- reordering inside a playlist ----
  const dropKey = $derived(`reorder:${playlistId ?? ""}`);
  const reordering = $derived(
    reorderable && dnd.payload?.kind === "tracks" && dnd.payload.fromPlaylist === playlistId && dnd.payload.itemIds?.length === 1 && dnd.target === dropKey,
  );
  // Rows all have the same height, so the drop position follows from the pointer alone, also
  // for rows that aren't rendered.
  const insertAt = $derived.by(() => {
    if (!reordering || !listEl) return -1;
    return insertIndex(dnd.y - listEl.getBoundingClientRect().top, rowHeight, tracks.length);
  });
  const lineTop = $derived(insertAt < 0 ? 0 : insertAt * rowHeight);

  async function dropReorder(p: Payload) {
    if (p.kind !== "tracks" || !playlistId || !p.itemIds || p.itemIds.length !== 1 || !itemIds) return;
    const from = itemIds.indexOf(p.itemIds[0]);
    let to = insertAt;
    if (from < 0 || to < 0) return;
    if (to > from) to -= 1;
    if (to === from) return;
    try {
      await api.playlistMove(playlistId, p.itemIds[0], to);
      onchanged?.();
    } catch (e) {
      fail(e);
    }
  }

  // Only a single song dragged within its own playlist reorders; anything else passes through.
  const target = $derived({
    key: dropKey,
    accepts: (p: Payload) => reorderable && p.kind === "tracks" && p.fromPlaylist === playlistId && p.itemIds?.length === 1,
    drop: dropReorder,
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") selected = new Set();
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a" && !(e.target instanceof HTMLInputElement)) {
      e.preventDefault();
      selected = new Set(tracks.map((_, i) => i));
    }
  }

  // Hidden songs ("Don't play") stay in their lists but look dimmed.
  const hiddenNow = $derived(library.hidden.length ? new Set(tracks.filter((x) => library.isHidden(x)).map((x) => x.id)) : null);
</script>

<svelte:window onkeydown={onKey} />

<div
  class="list"
  bind:this={listEl}
  use:dropTarget={target}
  style:padding-top={virtual ? `${range.start * rowHeight}px` : null}
  style:padding-bottom={virtual ? `${(tracks.length - range.end) * rowHeight}px` : null}>
  {#each indices as i (keys[i])}
    {@const track = tracks[i]}
    <div class="slot" animate:flip={{ duration: 300, easing: spring }}>
      <TrackRow
        {track}
        {compact}
        {showAlbum}
        selected={selected.has(i)}
        hidden={hiddenNow?.has(track.id) ?? false}
        onselect={(e) => select(e, i)}
        onactivate={() => activate(i)}
        onmenu={(e) => menuFor(e, i)}
        onpress={(e) => press(e, i)} />
    </div>
  {/each}
  {#if insertAt >= 0}<div class="line" style:top="{lineTop - 1}px"></div>{/if}
</div>

<script lang="ts" module>
  /** A soft spring: overshoots slightly, then settles (same curve as the queue panel). */
  function spring(x: number): number {
    const s = 0.9;
    const t = x - 1;
    return t * t * ((s + 1) * t + s) + 1;
  }
</script>

<style>
  .list {
    position: relative;
  }
  .line {
    position: absolute;
    left: 8px;
    right: 8px;
    height: 2px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.85);
    pointer-events: none;
  }
</style>
