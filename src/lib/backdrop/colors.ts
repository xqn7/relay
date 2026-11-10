// Picks the 2–3 colors that define a cover, then tones them for the backdrop: blues and
// purples are pulled toward neutral (design rule), and everything is kept dark enough that
// white text stays readable on top.

export type Rgb = [number, number, number];

const NEUTRAL: Rgb[] = [
  [58, 58, 62],
  [38, 38, 42],
  [74, 72, 70],
];

function rgbToHsl([r, g, b]: Rgb): [number, number, number] {
  const rn = r / 255, gn = g / 255, bn = b / 255;
  const max = Math.max(rn, gn, bn), min = Math.min(rn, gn, bn);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h = max === rn ? (gn - bn) / d + (gn < bn ? 6 : 0) : max === gn ? (bn - rn) / d + 2 : (rn - gn) / d + 4;
  h *= 60;
  return [h, s, l];
}

function hslToRgb(h: number, s: number, l: number): Rgb {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255)];
}

const distance = (a: Rgb, b: Rgb) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

/** Dominant colors from RGBA pixel data (a small, downscaled cover is plenty). */
export function dominantColors(data: Uint8ClampedArray, count = 3): Rgb[] {
  const buckets = new Map<number, { r: number; g: number; b: number; n: number }>();
  for (let i = 0; i < data.length; i += 4) {
    if (data[i + 3] < 128) continue;
    const key = ((data[i] >> 4) << 8) | ((data[i + 1] >> 4) << 4) | (data[i + 2] >> 4);
    const bucket = buckets.get(key) ?? { r: 0, g: 0, b: 0, n: 0 };
    bucket.r += data[i];
    bucket.g += data[i + 1];
    bucket.b += data[i + 2];
    bucket.n += 1;
    buckets.set(key, bucket);
  }
  const candidates = [...buckets.values()]
    .map((b) => {
      const rgb: Rgb = [b.r / b.n, b.g / b.n, b.b / b.n];
      const [, s, l] = rgbToHsl(rgb);
      // Colorful mid-tones say more about a cover than its black borders or white text.
      const extremes = l < 0.08 || l > 0.92 ? 0.15 : 1;
      return { rgb, score: b.n * (0.35 + s) * extremes };
    })
    .sort((a, b) => b.score - a.score);
  const picked: Rgb[] = [];
  for (const c of candidates) {
    if (picked.every((p) => distance(p, c.rgb) > 48)) picked.push(c.rgb);
    if (picked.length === count) break;
  }
  for (const c of candidates) {
    if (picked.length >= count) break;
    if (!picked.includes(c.rgb)) picked.push(c.rgb);
  }
  return picked.length ? picked : NEUTRAL.slice(0, count);
}

/** Makes a cover color suitable for the backdrop. */
export function toneForBackdrop(rgb: Rgb): Rgb {
  let [h, s, l] = rgbToHsl(rgb);
  // Blues through magenta-purples (the edge at 300° included, whatever the rounding).
  if (h >= 190 && h <= 315) s *= 0.3;
  s = Math.min(s, 0.7);
  l = Math.min(Math.max(l, 0.2), 0.46);
  return hslToRgb(h, s, l);
}

export function neutralColors(): Rgb[] {
  return NEUTRAL.slice();
}
