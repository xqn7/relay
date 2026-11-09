import { describe, expect, it } from "vitest";
import { insertIndex, visibleRange } from "./virtual";

describe("visibleRange", () => {
  it("renders the first screen plus overscan at the top", () => {
    expect(visibleRange(200, 800, 58, 5000, 10)).toEqual({ start: 0, end: 21 });
  });

  it("follows scrolling deep into a long list", () => {
    // Scrolled so the list top is 58,000 px above the viewport: row 1000 is at the top.
    expect(visibleRange(-58_000, 812, 58, 5000, 10)).toEqual({ start: 990, end: 1024 });
  });

  it("stays inside the list", () => {
    expect(visibleRange(-1_000_000, 800, 58, 100, 10)).toEqual({ start: 100, end: 100 });
    expect(visibleRange(5000, 800, 58, 100, 10)).toEqual({ start: 0, end: 0 });
    expect(visibleRange(0, 800, 58, 0, 10)).toEqual({ start: 0, end: 0 });
  });
});

describe("insertIndex", () => {
  it("snaps to the nearest gap between rows", () => {
    expect(insertIndex(10, 58, 5)).toBe(0);
    expect(insertIndex(40, 58, 5)).toBe(1);
    expect(insertIndex(-30, 58, 5)).toBe(0);
    expect(insertIndex(9999, 58, 5)).toBe(5);
  });
});
