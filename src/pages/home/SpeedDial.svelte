<!-- Pinned playlists, albums and artists. Drag a tile sideways to reorder; the others make
     room with the same spring as the queue. Songs dropped on a pinned playlist are added. -->
<script lang="ts">
  import { flip } from "svelte/animate";
  import type { Pin } from "../../lib/api";
  import Shelf from "../../lib/components/Shelf.svelte";
  import Tile from "../../lib/components/Tile.svelte";
  import { t } from "../../lib/i18n/index.svelte";
  import { spring } from "../../lib/motion";
  import { openPlaylistMenu } from "../../lib/playlistMenu";
  import { library } from "../../lib/stores/library.svelte";
  import { menu } from "../../lib/stores/menu.svelte";
  import { pinKey, speedDial } from "../../lib/stores/speedDial.svelte";
  import { ui } from "../../lib/stores/ui.svelte";
  import { playlistDrop } from "../libraryDrag";

  const DRAG_THRESHOLD = 6;

  interface DialTile {
    key: string;
    pin: Pin;
    title: string;
    sub: string;
    cover: string | null;
  }

  // Playlists show their current name and cover; pins of deleted playlists are left out.
  const tiles = $derived(
    speedDial.pins.flatMap((pin): DialTile[] => {
      const key = pinKey(pin);
      if (pin.kind !== "playlist") return [{ key, pin, title: pin.title, sub: pin.subtitle, cover: pin.cover }];
      const p = library.overview.playlists.find((x) => x.id === pin.id);
      return p ? [{ key, pin, title: p.name, sub: t("library.songs", { n: p.count }), cover: p.cover }] : [];
    }),
  );

  let drag = $state<{ key: string; startX: number; offset: number; width: number; moved: boolean } | null>(null);
  /** Set when a drag just ended, so the click that follows the release doesn't open the tile. */
  let justDragged = false;

  function press(e: PointerEvent, key: string) {
    if (e.button !== 0) return;
    const el = (e.currentTarget as HTMLElement).closest<HTMLElement>(".dial")!;
    drag = { key, startX: e.clientX, offset: 0, width: el.getBoundingClientRect().width + 4, moved: false };
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    let offset = e.clientX - drag.startX;
    if (!drag.moved && Math.abs(offset) < DRAG_THRESHOLD) return;
    drag.moved = true;
    const from = speedDial.pins.findIndex((p) => pinKey(p) === drag!.key);
    const to = Math.min(speedDial.pins.length - 1, Math.max(0, from + Math.round(offset / drag.width)));
    if (to !== from) {
      speedDial.reorder(from, to);
      // The tile moved in the DOM; shift the origin so it stays under the pointer.
      drag.startX += (to - from) * drag.width;
      offset = e.clientX - drag.startX;
    }
    drag.offset = offset;
  }

  function end() {
    if (drag?.moved) {
      speedDial.saveOrder();
      justDragged = true;
      setTimeout(() => (justDragged = false));
    }
    drag = null;
  }

  function open(tile: DialTile) {
    if (justDragged) return;
    const { pin } = tile;
    if (pin.kind === "playlist") ui.open({ name: "playlist", id: pin.id });
    else if (pin.source === "youtube" || pin.source === "soundcloud") {
      if (pin.kind === "album") ui.open({ name: "album", source: pin.source, id: pin.id });
      else ui.open({ name: "artist", source: pin.source, id: pin.id, artistName: pin.title });
    }
  }

  function onMenu(e: MouseEvent, tile: DialTile) {
    const p = tile.pin.kind === "playlist" ? library.overview.playlists.find((x) => x.id === tile.pin.id) : null;
    if (p) openPlaylistMenu(e, p);
    else menu.show(e, [speedDial.menuItem(tile.pin)]);
  }
</script>

<svelte:window onpointermove={move} onpointerup={end} onpointercancel={end} />

{#if tiles.length}
  <Shelf title={t("dial.title")}>
    {#each tiles as tile (tile.key)}
      <div
        class="dial"
        class:lifted={drag?.moved && drag.key === tile.key}
        style:transform={drag?.moved && drag.key === tile.key ? `translateX(${drag.offset}px)` : undefined}
        animate:flip={{ duration: drag?.key === tile.key ? 0 : 320, easing: spring }}>
        <Tile
          title={tile.title}
          subtitle={tile.sub}
          cover={tile.cover}
          round={tile.pin.kind === "artist"}
          icon={tile.pin.kind === "artist" ? "user" : tile.pin.kind === "album" ? "disc" : "list"}
          drop={tile.pin.kind === "playlist" ? playlistDrop(tile.pin.id) : undefined}
          onclick={() => open(tile)}
          onmenu={(e) => onMenu(e, tile)}
          onpress={(e) => press(e, tile.key)} />
      </div>
    {/each}
  </Shelf>
{/if}

<style>
  .dial {
    min-width: 0;
  }
  .lifted {
    position: relative;
    z-index: 2;
    opacity: 0.92;
  }
</style>
