// The 10-band equalizer: band frequencies, built-in presets and the shape of a saved EQ.

export const BANDS = [32, 64, 125, 250, 500, 1000, 2000, 4000, 8000, 16000] as const;
export const MAX_DB = 12;

export interface EqState {
  on: boolean;
  /** A built-in preset id, "user:<name>" for a saved one, or "custom" for unsaved changes. */
  preset: string;
  gains: number[];
}

export interface UserPreset {
  name: string;
  gains: number[];
}

export const BUILT_IN: Record<string, number[]> = {
  flat: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  bass: [6, 5, 4, 2, 0, 0, 0, 0, 0, 0],
  bassReducer: [-6, -5, -4, -2, 0, 0, 0, 0, 0, 0],
  treble: [0, 0, 0, 0, 0, 0, 2, 4, 5, 6],
  vocal: [-2, -2, -1, 1, 3, 4, 3, 1, 0, -1],
  rock: [4, 3, 2, 0, -1, -1, 1, 2, 3, 4],
  pop: [-1, 0, 2, 3, 3, 2, 0, -1, -1, -1],
  electronic: [4, 4, 1, 0, -2, 1, 0, 1, 3, 4],
  hipHop: [5, 4, 1, 2, -1, -1, 1, 0, 1, 2],
  jazz: [3, 2, 1, 2, -1, -1, 0, 1, 2, 3],
  classical: [4, 3, 2, 1, -1, -1, 0, 2, 3, 4],
  acoustic: [3, 3, 2, 1, 1, 1, 2, 2, 2, 1],
  loudness: [5, 3, 0, 0, -1, 0, -1, -2, 3, 1],
};
export const BUILT_IN_IDS = Object.keys(BUILT_IN);

export const flatEq = (): EqState => ({ on: false, preset: "flat", gains: [...BUILT_IN.flat] });

export function validGains(g: unknown): g is number[] {
  return Array.isArray(g) && g.length === BANDS.length && g.every((x) => typeof x === "number" && Number.isFinite(x) && Math.abs(x) <= MAX_DB);
}

export function validEq(v: unknown): v is EqState {
  if (typeof v !== "object" || v === null) return false;
  const e = v as Partial<EqState>;
  return typeof e.on === "boolean" && typeof e.preset === "string" && e.preset.length < 80 && validGains(e.gains);
}

export function validUserPresets(v: unknown): v is UserPreset[] {
  return (
    Array.isArray(v) &&
    v.length <= 50 &&
    v.every((p) => typeof p === "object" && p !== null && typeof p.name === "string" && p.name.length > 0 && p.name.length <= 40 && validGains(p.gains))
  );
}

/** The gains a preset id stands for, or null when it no longer exists. */
export function presetGains(id: string, user: UserPreset[]): number[] | null {
  if (id in BUILT_IN) return [...BUILT_IN[id]];
  if (id.startsWith("user:")) {
    const found = user.find((p) => p.name === id.slice(5));
    return found ? [...found.gains] : null;
  }
  return null;
}

/** Lowers the whole signal by the biggest boost, so boosted bands can't clip. */
export function preampDb(gains: number[]): number {
  return -Math.max(0, ...gains);
}

/** Band frequencies as short labels: "32", "1k", "16k". */
export function bandLabel(hz: number): string {
  return hz >= 1000 ? `${hz / 1000}k` : String(hz);
}

/** Pitch in semitones as a playback-rate factor for the shifter. */
export function semitoneRatio(semitones: number): number {
  return Math.pow(2, semitones / 12);
}
