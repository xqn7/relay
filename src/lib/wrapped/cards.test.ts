import { describe, expect, it } from "vitest";
import type { Stats, Track } from "../api";
import { buildCards, leadCover, monthPeriod, yearPeriod } from "./cards";

const t = (key: string, vars?: Record<string, string | number>) => `${key}${vars ? JSON.stringify(vars) : ""}`;

const track = (id: string): Track => ({
  id: `youtube:${id}`,
  source: "youtube",
  sourceId: id,
  title: `Song ${id}`,
  artists: [{ id: null, name: "Artist" }],
  album: null,
  durationMs: 1000,
  cover: `https://x.test/${id}.jpg`,
  isVideo: false,
  link: null,
});

const empty: Stats = {
  minutes: 0,
  plays: 0,
  songs: 0,
  artists: 0,
  topSongs: [],
  topArtists: [],
  topDay: null,
  longestStreak: 0,
  streakStart: null,
  firstPlay: null,
};

describe("wrapped periods", () => {
  it("covers whole local months, also across the year end", () => {
    const p = monthPeriod(new Date(2026, 0, 15), -1, "en");
    expect(new Date(p.from * 1000)).toEqual(new Date(2025, 11, 1));
    expect(new Date(p.to * 1000)).toEqual(new Date(2026, 0, 1));
    expect(p.label).toBe("December 2025");
  });

  it("covers whole years", () => {
    const p = yearPeriod(new Date(2026, 5, 1), 0);
    expect(new Date(p.from * 1000)).toEqual(new Date(2026, 0, 1));
    expect(new Date(p.to * 1000)).toEqual(new Date(2027, 0, 1));
    expect(p.label).toBe("2026");
  });
});

describe("wrapped cards", () => {
  const period = yearPeriod(new Date(2026, 5, 1), 0);

  it("only shows what there is", () => {
    expect(buildCards(empty, period, "en", t).map((c) => c.id)).toEqual(["intro", "minutes"]);
  });

  it("builds every card from full stats", () => {
    const s: Stats = {
      ...empty,
      minutes: 1234,
      plays: 300,
      songs: 80,
      artists: 20,
      topSongs: [{ track: track("a"), plays: 12 }],
      topArtists: [{ name: "Artist", plays: 40, minutes: 150, cover: null }],
      topDay: { day: new Date(2026, 2, 3).getTime() / 1000, minutes: 95 },
      longestStreak: 9,
      streakStart: new Date(2026, 1, 1).getTime() / 1000,
    };
    const cards = buildCards(s, period, "en", t);
    expect(cards.map((c) => c.id)).toEqual(["intro", "minutes", "songs", "artists", "day", "streak"]);
    expect(cards[1].big).toBe("1,234");
    expect(cards[2].rows?.[0].value).toContain('"count":"12"');
    expect(cards[4].big).toBe("March 3");
    expect(leadCover(s)).toBe("https://x.test/a.jpg");
  });

  it("leaves out a one-day streak", () => {
    const cards = buildCards({ ...empty, longestStreak: 1, streakStart: 0 }, period, "en", t);
    expect(cards.some((c) => c.id === "streak")).toBe(false);
  });
});
