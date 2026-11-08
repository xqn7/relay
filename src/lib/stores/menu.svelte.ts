// One context menu for the whole app, opened from right-clicks and "⋮" buttons.
import type { IconName } from "../icons";

export interface MenuItem {
  label: string;
  icon: IconName;
  action: () => void;
  disabled?: boolean;
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

  close(): void {
    this.open = false;
  }
}

export const menu = new MenuStore();
