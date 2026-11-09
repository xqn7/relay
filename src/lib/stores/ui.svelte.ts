// Navigation, overlays and toasts.
import type { IconName } from "../icons";

import type { SearchSource } from "../api";

export type Route =
  | { name: "home" }
  | { name: "library"; tab?: LibraryTab }
  | { name: "playlist"; id: string }
  | { name: "liked" }
  | { name: "local" }
  | { name: "folder"; id: string }
  | { name: "trash" }
  | { name: "album"; source: SearchSource; id: string }
  | { name: "artist"; source: SearchSource; id: string; artistName: string };

export type LibraryTab = "all" | "playlists" | "artists";

interface Entry {
  route: Route;
  key: number;
  scroll: number;
}

export interface Toast {
  id: number;
  text: string;
  icon?: IconName;
  /** 0..1 for a progress bar; the toast stays until progress is cleared. */
  progress?: number;
  /** A button on the toast, e.g. "Undo". */
  action?: { label: string; run: () => void };
}

const TOAST_MS = 3600;
let nextId = 1;

class UiStore {
  private stack = $state<Entry[]>([{ route: { name: "home" }, key: 0, scroll: 0 }]);
  private pos = $state(0);
  /** -1 when going back, 1 otherwise; page transitions slide in this direction. */
  direction = $state<1 | -1>(1);
  searchOpen = $state(false);
  settingsOpen = $state(false);
  queueOpen = $state(false);
  /** Files are being dragged over the window from outside. */
  filesOver = $state(false);
  /** Art of the open artist/album page; the backdrop takes its colors while it's open. */
  heroCover = $state<string | null>(null);
  toasts = $state<Toast[]>([]);

  get entry(): Entry {
    return this.stack[this.pos];
  }
  get route(): Route {
    return this.entry.route;
  }
  get canBack(): boolean {
    return this.pos > 0;
  }
  get canForward(): boolean {
    return this.pos < this.stack.length - 1;
  }

  /** Set by the shell: reads the page's scroll position, remembered when leaving a page. */
  scrollTop: () => number = () => 0;

  /** Goes to a page from anywhere (links, menus, tiles). */
  open(route: Route): void {
    this.searchOpen = false;
    this.go(route, this.scrollTop());
  }

  /** Replaces the current page without adding history (e.g. after its playlist was deleted). */
  replace(route: Route): void {
    this.stack[this.pos] = { route, key: nextId++, scroll: 0 };
  }

  go(route: Route, scrollFrom: number): void {
    if (JSON.stringify(route) === JSON.stringify(this.route)) return;
    this.entry.scroll = scrollFrom;
    this.stack = [...this.stack.slice(0, this.pos + 1), { route, key: nextId++, scroll: 0 }];
    this.pos = this.stack.length - 1;
    this.direction = 1;
  }

  back(scrollFrom: number): void {
    if (!this.canBack) return;
    this.entry.scroll = scrollFrom;
    this.direction = -1;
    this.pos -= 1;
  }

  forward(scrollFrom: number): void {
    if (!this.canForward) return;
    this.entry.scroll = scrollFrom;
    this.direction = 1;
    this.pos += 1;
  }

  resetNav(): void {
    this.stack = [{ route: { name: "home" }, key: nextId++, scroll: 0 }];
    this.pos = 0;
    this.searchOpen = this.settingsOpen = this.queueOpen = false;
  }

  toast(text: string, icon?: IconName, action?: Toast["action"]): number {
    const id = nextId++;
    this.toasts = [...this.toasts.slice(-2), { id, text, icon, action }];
    // Toasts with a button stay a little longer, so there's time to reach it.
    setTimeout(() => this.dismiss(id), action ? TOAST_MS * 2 : TOAST_MS);
    return id;
  }

  /** A toast with a progress bar, updated in place; call with null progress to finish it. */
  progressToast(id: number | null, text: string, progress: number | null, action?: Toast["action"]): number {
    if (id != null && this.toasts.some((t) => t.id === id)) {
      this.toasts = this.toasts.map((t) => (t.id === id ? { ...t, text, progress: progress ?? undefined, action } : t));
      if (progress == null) setTimeout(() => this.dismiss(id), action ? TOAST_MS * 2 : TOAST_MS);
      return id;
    }
    const newId = nextId++;
    this.toasts = [...this.toasts.slice(-2), { id: newId, text, progress: progress ?? undefined, action }];
    if (progress == null) setTimeout(() => this.dismiss(newId), action ? TOAST_MS * 2 : TOAST_MS);
    return newId;
  }

  dismiss(id: number): void {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }
}

export const ui = new UiStore();
