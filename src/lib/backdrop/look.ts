// How a custom background image is shown. Stored per image as JSON; anything malformed falls
// back to the defaults, never trusted as is.

export interface Look {
  /** px */
  blur: number;
  /** 0 = untouched, 0.85 = almost black */
  dim: number;
  brightness: number;
  saturation: number;
  /** 1 = the image just covers the window */
  zoom: number;
  /** Which part shows when the image is bigger than the window, in % (50 = center). */
  x: number;
  y: number;
  /** Tint the image with the current song's cover colors. */
  mix: boolean;
}

export const LOOK_RANGES = {
  blur: [0, 40],
  dim: [0, 0.85],
  brightness: [0.5, 1.5],
  saturation: [0, 2],
  zoom: [1, 3],
  x: [0, 100],
  y: [0, 100],
} as const satisfies Record<string, readonly [number, number]>;

export type LookNumber = keyof typeof LOOK_RANGES;

export const defaultLook: Look = { blur: 10, dim: 0.45, brightness: 1, saturation: 1, zoom: 1, x: 50, y: 50, mix: false };

export function readLook(raw: unknown): Look {
  const r = typeof raw === "object" && raw !== null && !Array.isArray(raw) ? (raw as Record<string, unknown>) : {};
  const look: Look = { ...defaultLook, mix: r.mix === true };
  for (const key of Object.keys(LOOK_RANGES) as LookNumber[]) {
    const v = r[key];
    const [lo, hi] = LOOK_RANGES[key];
    if (typeof v === "number" && Number.isFinite(v)) look[key] = Math.min(hi, Math.max(lo, v));
  }
  return look;
}

/** The CSS filter for a look. */
export function lookFilter(l: Look): string {
  return `blur(${l.blur}px) brightness(${l.brightness}) saturate(${l.saturation})`;
}
