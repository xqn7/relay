// One context menu for the whole app, opened from right-clicks and "⋮" buttons.
import type { IconName } from "../icons";

export interface MenuItem {
  label: string;
  icon: IconName;
  action: () => void;
  disabled?: boolean;
  /** Draws a thin line above this item, to group related entries. */
  divider?: boolean;
  /** Opens another list in place (e.g. "Add to playlist") instead of closing the menu. */
  submenu?: boolean;
}

class MenuStore {
  items = $state<MenuItem[]>([]);
  x = $state(0);
  y = $state(0);
  open = $state(false);

  show(e: MouseEvent, items: MenuItem[]): void {
    e.preventDefault();
    e.stopPropagation();
    this.items = items;
    // A "⋮" button pressed with the keyboard reports no position; the menu opens under it then.
    const box = e.clientX === 0 && e.clientY === 0 && e.target instanceof Element ? e.target.getBoundingClientRect() : null;
    this.x = box ? box.left : e.clientX;
    this.y = box ? box.bottom + 4 : e.clientY;
    this.open = true;
  }

  /** Swaps in a second list at the same spot (for "Add to playlist" and "Go to artist"). */
  replace(items: MenuItem[]): void {
    this.items = items;
  }

  close(): void {
    this.open = false;
  }
}

export const menu = new MenuStore();
