import { describe, expect, it } from "vitest";
import { dominantColors, toneForBackdrop, type Rgb } from "./colors";

function image(colors: [Rgb, number][]): Uint8ClampedArray {
  const px: number[] = [];
  for (const [[r, g, b], n] of colors) for (let i = 0; i < n; i++) px.push(r, g, b, 255);
  return new Uint8ClampedArray(px);
}

describe("cover colors", () => {
  it("finds the main colors and prefers color over black borders", () => {
    const data = image([
      [[0, 0, 0], 500],
      [[200, 40, 40], 200],
      [[230, 180, 40], 120],
    ]);
    const [first, second] = dominantColors(data, 3);
    expect(first[0]).toBeGreaterThan(150);
    expect(first[1]).toBeLessThan(80);
    expect(second[1]).toBeGreaterThan(150);
  });

  it("tones blue and purple down toward neutral", () => {
    const [r, g, b] = toneForBackdrop([40, 60, 230]);
    expect(b - Math.min(r, g)).toBeLessThan(60);
  });

  it("keeps warm colors warm but dark enough for white text", () => {
    const [r, g, b] = toneForBackdrop([250, 120, 40]);
    expect(r).toBeGreaterThan(g);
    expect(r).toBeGreaterThan(b);
    expect(Math.max(r, g, b)).toBeLessThan(220);
  });

  it("falls back to neutral greys for empty images", () => {
    expect(dominantColors(new Uint8ClampedArray(0), 3)).toHaveLength(3);
  });
});
