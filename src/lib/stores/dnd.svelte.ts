// Drag and drop inside the app, built on pointer events. (The window's native drag-and-drop
// is reserved for files dropped from Explorer, which go to Local music.) Drop targets register
// with the `dropTarget` action; the place under the pointer reacts while something is dragged.
import type { Track } from "../api";

export type Payload =
  | { kind: "tracks"; tracks: Track[]; label: string; fromPlaylist?: string; itemIds?: string[] }
  | { kind: "playlist"; id: string; label: string }
  | { kind: "folder"; id: string; label: string };

export interface DropHandler {
  accepts: (p: Payload) => boolean;
  drop: (p: Payload, x: number, y: number) => void;
}

/** Pixels the pointer must travel before a press becomes a drag (so clicks stay clicks). */
const THRESHOLD = 6;

class DndStore {
  payload = $state<Payload | null>(null);
  x = $state(0);
  y = $state(0);
  /** Key of the drop target under the pointer that accepts the payload. */
  target = $state<string | null>(null);

  private handlers = new Map<string, DropHandler>();
  private armed: { make: () => Payload; x: number; y: number } | null = null;

  /** Call on pointerdown; the drag starts only once the pointer has moved a little. */
  arm(e: PointerEvent, make: () => Payload): void {
    if (e.button !== 0) return;
    this.armed = { make, x: e.clientX, y: e.clientY };
    window.addEventListener("pointermove", this.onMove);
    window.addEventListener("pointerup", this.onUp);
    window.addEventListener("pointercancel", this.onCancel);
  }

  register(key: string, handler: DropHandler): () => void {
    this.handlers.set(key, handler);
    return () => {
      if (this.handlers.get(key) === handler) this.handlers.delete(key);
    };
  }

  private onMove = (e: PointerEvent) => {
    if (!this.payload) {
      if (!this.armed || Math.hypot(e.clientX - this.armed.x, e.clientY - this.armed.y) < THRESHOLD) return;
      this.payload = this.armed.make();
      document.documentElement.classList.add("dragging");
    }
    this.x = e.clientX;
    this.y = e.clientY;
    this.target = this.targetAt(e.clientX, e.clientY);
  };

  private onUp = (e: PointerEvent) => {
    const payload = this.payload;
    const key = payload ? this.targetAt(e.clientX, e.clientY) : null;
    this.stop();
    if (payload && key) this.handlers.get(key)?.drop(payload, e.clientX, e.clientY);
  };

  private onCancel = () => this.stop();

  private targetAt(x: number, y: number): string | null {
    if (!this.payload) return null;
    let el = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
    while (el) {
      const key = el.dataset.drop!;
      if (this.handlers.get(key)?.accepts(this.payload)) return key;
      el = el.parentElement?.closest<HTMLElement>("[data-drop]") ?? null;
    }
    return null;
  }

  private stop(): void {
    this.armed = null;
    this.payload = null;
    this.target = null;
    document.documentElement.classList.remove("dragging");
    window.removeEventListener("pointermove", this.onMove);
    window.removeEventListener("pointerup", this.onUp);
    window.removeEventListener("pointercancel", this.onCancel);
  }
}

export const dnd = new DndStore();

/** Svelte action: marks an element as a drop target. */
export function dropTarget(node: HTMLElement, params: { key: string } & DropHandler) {
  let off = () => {};
  const apply = (p: { key: string } & DropHandler) => {
    off();
    node.dataset.drop = p.key;
    off = dnd.register(p.key, p);
  };
  apply(params);
  return {
    update: apply,
    destroy: () => off(),
  };
}
