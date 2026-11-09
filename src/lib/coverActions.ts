// Saving covers: "Save image" in menus, and dragging a cover out of the window onto the
// desktop or into Explorer (a real file drag, started by the backend).
import { api, artistLine, type Track } from "./api";
import { t } from "./i18n/index.svelte";
import { fail } from "./stores/library.svelte";
import { menu, type MenuItem } from "./stores/menu.svelte";
import { ui } from "./stores/ui.svelte";

const DRAG_THRESHOLD_PX = 6;

export async function saveCover(url: string, name: string): Promise<void> {
  try {
    if (await api.coverSave(url, name)) ui.toast(t("cover.saved"), "check");
  } catch (e) {
    fail(e);
  }
}

/** A song's cover at the best size we know of. */
export async function saveTrackCover(track: Track): Promise<void> {
  if (!track.cover) return;
  const hd = await api.coverHd(track).catch(() => null);
  await saveCover(hd ?? track.cover, `${artistLine(track)} - ${track.title}`);
}

export function coverMenuItems(url: string, name: string): MenuItem[] {
  return [{ label: t("cover.save"), icon: "image", action: () => saveCover(url, name) }];
}

export function openCoverMenu(e: MouseEvent, url: string | null, name: string): void {
  if (!url) return;
  menu.show(e, coverMenuItems(url, name));
}

/** Svelte action: drag the cover out as an image file. The file is fetched on press, so the
 * drag can start the moment the pointer moves. */
export function coverDrag(node: HTMLElement, params: { url: string | null; name: string }) {
  let current = params;
  let start: { x: number; y: number } | null = null;
  let ready: Promise<boolean> | null = null;

  function down(e: PointerEvent) {
    if (e.button !== 0 || !current.url) return;
    start = { x: e.clientX, y: e.clientY };
    ready = api
      .coverDragPrepare(current.url, current.name)
      .then(() => true)
      .catch(() => false);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up, { once: true });
  }

  async function move(e: PointerEvent) {
    if (!start || !ready || Math.hypot(e.clientX - start.x, e.clientY - start.y) < DRAG_THRESHOLD_PX) return;
    const pending = ready;
    up();
    if ((await pending) && e.buttons & 1) api.coverDragStart().catch(() => {});
  }

  function up() {
    start = null;
    ready = null;
    window.removeEventListener("pointermove", move);
  }

  // The browser's own image drag would only show a ghost that can't be dropped anywhere.
  const noNativeDrag = (e: DragEvent) => e.preventDefault();
  node.addEventListener("pointerdown", down);
  node.addEventListener("dragstart", noNativeDrag);
  return {
    update(next: { url: string | null; name: string }) {
      current = next;
    },
    destroy() {
      up();
      node.removeEventListener("pointerdown", down);
      node.removeEventListener("dragstart", noNativeDrag);
    },
  };
}
