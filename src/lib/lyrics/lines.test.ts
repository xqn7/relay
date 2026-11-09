import { describe, expect, it } from "vitest";
import { activeIndex, buildLines, splitMatches, timeWords, wordProgress } from "./lines";

describe("lyrics lines", () => {
  it("adds ♪ for a late start, empty lines and long pauses", () => {
    const lines = buildLines(
      [
        { timeMs: 8000, text: "first line here" },
        { timeMs: 11000, text: "" },
        { timeMs: 20000, text: "after a break" },
        { timeMs: 40000, text: "end" },
      ],
      60000,
    );
    expect(lines.map((l) => (l.gap ? "♪" : l.text))).toEqual(["♪", "first line here", "♪", "after a break", "♪", "end"]);
    expect(lines[0].timeMs).toBe(0);
    expect(lines[3].index).toBe(2);
  });

  it("drops short empty lines and needs no marker for a quick start", () => {
    const lines = buildLines(
      [
        { timeMs: 500, text: "a" },
        { timeMs: 2000, text: "" },
        { timeMs: 3000, text: "b" },
      ],
      5000,
    );
    expect(lines.map((l) => l.text)).toEqual(["a", "b"]);
  });

  it("spreads a line over its words and never past the next line", () => {
    const words = timeWords("one three", 1000, 1000);
    expect(words[0]).toEqual({ text: "one", startMs: 1000, endMs: 1400 });
    expect(words[1].endMs).toBe(2000);
    const [line] = buildLines([{ timeMs: 0, text: "a very long line that keeps going" }, { timeMs: 800, text: "x" }], 3000);
    expect(line.endMs).toBe(800);
    expect(wordProgress(words[0], 1200)).toBeCloseTo(0.5);
    expect(wordProgress(words[1], 900)).toBe(0);
  });

  it("finds the line being sung", () => {
    const lines = buildLines([{ timeMs: 1000, text: "a" }, { timeMs: 3000, text: "b" }, { timeMs: 5000, text: "c" }], 9000);
    expect(activeIndex(lines, 500)).toBe(-1);
    expect(activeIndex(lines, 3000)).toBe(1);
    expect(activeIndex(lines, 99999)).toBe(2);
  });

  it("highlights search hits ignoring case and accents", () => {
    expect(splitMatches("Café, CAFE and more", "cafe")).toEqual([
      { text: "Café", hit: true },
      { text: ", ", hit: false },
      { text: "CAFE", hit: true },
      { text: " and more", hit: false },
    ]);
    expect(splitMatches("nothing", "  ")).toEqual([{ text: "nothing", hit: false }]);
  });
});
