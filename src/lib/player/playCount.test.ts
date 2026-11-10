import { describe, expect, it } from "vitest";
import { PlayCounter } from "./playCount";

function run(c: PlayCounter, key: string, from: number, to: number, dur: number): number {
  let hits = 0;
  for (let p = from; p <= to; p += 250) if (c.tick(key, p, dur)) hits++;
  return hits;
}

describe("play counter", () => {
  it("counts once after 30 seconds of listening", () => {
    const c = new PlayCounter();
    c.start("a");
    expect(run(c, "a", 0, 29_000, 200_000)).toBe(0);
    expect(run(c, "a", 29_250, 120_000, 200_000)).toBe(1);
    expect(c.listenedMs).toBeGreaterThan(119_000);
  });

  it("counts half of a short song", () => {
    const c = new PlayCounter();
    c.start("a");
    expect(run(c, "a", 0, 21_000, 40_000)).toBe(1);
  });

  it("doesn't count seeking", () => {
    const c = new PlayCounter();
    c.start("a");
    run(c, "a", 0, 5_000, 200_000);
    run(c, "a", 150_000, 160_000, 200_000);
    expect(c.counted).toBe(false);
    expect(c.listenedMs).toBe(15_000);
  });

  it("ignores reports for another playback and starts over", () => {
    const c = new PlayCounter();
    c.start("a");
    expect(run(c, "b", 0, 60_000, 200_000)).toBe(0);
    run(c, "a", 0, 40_000, 200_000);
    c.start("a");
    expect(c.counted).toBe(false);
    expect(run(c, "a", 0, 31_000, 200_000)).toBe(1);
  });
});
