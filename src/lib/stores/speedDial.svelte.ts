// Speed dial: pinned playlists, albums and artists on Home, in the user's own order.
import { api, type Pin } from "../api";
import { t } from "../i18n/index.svelte";
import type { MenuItem } from "./menu.svelte";
import { fail } from "./library.svelte";
import { ui } from "./ui.svelte";

export const pinKey = (p: Pick<Pin, "kind" | "source" | "id">): string => `${p.kind}:${p.source}:${p.id}`;

class SpeedDialStore {
  pins = $state<Pin[]>([]);

  async refresh(): Promise<void> {
    this.pins = await api.speedDialList();
  }

  reset(): void {
    this.pins = [];
  }

  isPinned(p: Pick<Pin, "kind" | "source" | "id">): boolean {
    const key = pinKey(p);
    return this.pins.some((x) => pinKey(x) === key);
  }

  async toggle(pin: Pin): Promise<void> {
    const on = !this.isPinned(pin);
    try {
      await api.speedDialSet(pin, on);
      await this.refresh();
      ui.toast(on ? t("dial.pinned", { name: pin.title }) : t("dial.unpinned", { name: pin.title }), "pin");
    } catch (e) {
      const full = typeof e === "object" && e !== null && "code" in e && e.code === "too_many";
      if (full) ui.toast(t("dial.full"), "alert");
      else fail(e);
    }
  }

  /** Moves a pin while it's being dragged; saved with `saveOrder` on drop. */
  reorder(from: number, to: number): void {
    if (from === to || from < 0 || to < 0 || from >= this.pins.length || to >= this.pins.length) return;
    const next = this.pins.slice();
    const [p] = next.splice(from, 1);
    next.splice(to, 0, p);
    this.pins = next;
  }

  saveOrder(): void {
    api.speedDialOrder(this.pins.map(pinKey)).catch(fail);
  }

  menuItem(pin: Pin): MenuItem {
    const pinned = this.isPinned(pin);
    return { label: pinned ? t("dial.unpin") : t("dial.pin"), icon: pinned ? "pinOff" : "pin", action: () => this.toggle(pin) };
  }
}

export const speedDial = new SpeedDialStore();
