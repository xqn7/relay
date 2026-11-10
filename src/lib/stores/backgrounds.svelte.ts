// Saved background images and which one is used. The files live in the app's data folder;
// how each one looks (blur, dim, ...) is stored with it.
import { api, isAppError, type Background } from "../api";
import { readLook, type Look } from "../backdrop/look";
import { t } from "../i18n/index.svelte";
import { fail } from "./library.svelte";
import { settings, type BackgroundPlace } from "./settings.svelte";
import { ui } from "./ui.svelte";

const SAVE_DELAY_MS = 400;
const ACCEPTED = ["image/jpeg", "image/png", "image/webp", "image/gif"];

class BackgroundsStore {
  list = $state<Background[]>([]);
  private saveTimers = new Map<string, number>();

  async refresh(): Promise<void> {
    this.list = await api.backgroundsList();
  }

  reset(): void {
    this.list = [];
  }

  /** The chosen image, when Background is set to "Custom image" and the image still exists. */
  get active(): Background | null {
    if (settings.values.background !== "image") return null;
    return this.list.find((b) => b.id === settings.values.backgroundId) ?? null;
  }

  /** The image to draw in a place (whole app, fullscreen player, idle screen), if any. */
  activeIn(place: BackgroundPlace): Background | null {
    return settings.values.backgroundWhere.includes(place) ? this.active : null;
  }

  look(b: Background): Look {
    return readLook(b.look);
  }

  async add(file: File): Promise<void> {
    if (!ACCEPTED.includes(file.type)) {
      ui.toast(t("bg.badFile"), "alert");
      return;
    }
    try {
      const added = await api.backgroundAdd(new Uint8Array(await file.arrayBuffer()));
      this.list = [...this.list, added];
      this.use(added.id);
    } catch (e) {
      const code = isAppError(e) ? e.code : "";
      if (code === "too_large") ui.toast(t("bg.tooLarge"), "alert");
      else if (code === "too_many") ui.toast(t("bg.tooMany"), "alert");
      else if (code === "bad_file") ui.toast(t("bg.badFile"), "alert");
      else fail(e);
    }
  }

  use(id: string): void {
    settings.set("backgroundId", id);
    settings.set("background", "image");
  }

  /** Changes show at once; saving waits until the slider rests. */
  setLook(id: string, look: Look): void {
    this.list = this.list.map((b) => (b.id === id ? { ...b, look } : b));
    clearTimeout(this.saveTimers.get(id));
    this.saveTimers.set(id, window.setTimeout(() => api.backgroundLookSet(id, look).catch(fail), SAVE_DELAY_MS));
  }

  async remove(id: string): Promise<void> {
    try {
      await api.backgroundDelete(id);
      this.list = this.list.filter((b) => b.id !== id);
      if (settings.values.backgroundId === id) {
        const next = this.list[0];
        if (next) settings.set("backgroundId", next.id);
        else settings.set("background", "cover");
      }
    } catch (e) {
      fail(e);
    }
  }
}

export const backgrounds = new BackgroundsStore();
