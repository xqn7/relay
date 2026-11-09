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
    this.x = e.clientX;
    this.y = e.clientY;
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
