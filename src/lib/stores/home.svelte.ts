// What Home shows: recently and most played songs, today's mixes and Discover's artists.
// Loaded when Home (or Discover, or a mix) opens; lists refresh after new plays.
import { api, type HomeLists, type Mix, type Suggestion } from "../api";

/** Today's date on this PC, e.g. "2026-11-10": mixes are made once per local day. */
export function localDay(d = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

class HomeStore {
  lists = $state<HomeLists | null>(null);
  mixes = $state<Mix[] | null>(null);
  mixesFailed = $state(false);
  suggestions = $state<Suggestion[] | null>(null);
  discoverFailed = $state(false);
  private mixesDay = "";
  private mixesRun: Promise<void> | null = null;

  async loadLists(): Promise<void> {
    this.lists = await api.homeLists();
  }

  /** Making mixes takes a few seconds; Home and a mix page opening together share one run. */
  loadMixes(): Promise<void> {
    const today = localDay();
    // No mixes yet is asked again: a few more plays today can be enough for the first ones.
    if (this.mixes?.length && this.mixesDay === today) return Promise.resolve();
    this.mixesRun ??= this.fetchMixes(today).finally(() => (this.mixesRun = null));
    return this.mixesRun;
  }

  private async fetchMixes(today: string): Promise<void> {
    this.mixesFailed = false;
    try {
      this.mixes = await api.homeMixes(today);
      this.mixesDay = today;
    } catch (e) {
      console.error("mixes failed", e);
      this.mixesFailed = true;
    }
  }

  async loadDiscover(): Promise<void> {
    this.discoverFailed = false;
    try {
      this.suggestions = await api.discover();
    } catch (e) {
      console.error("discover failed", e);
      this.discoverFailed = true;
    }
  }

  reset(): void {
    this.lists = this.mixes = this.suggestions = null;
    this.mixesDay = "";
  }
}

export const home = new HomeStore();
