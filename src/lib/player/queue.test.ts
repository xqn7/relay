import { describe, expect, it } from "vitest";
import type { Track } from "../api";
import * as Q from "./queue";

const track = (n: number): Track => ({
  id: `youtube:id${n}`,
  source: "youtube",
  sourceId: `id${n}`,
  title: `Song ${n}`,
  artists: [{ id: null, name: "Artist" }],
  album: null,
  durationMs: 180000,
  cover: null,
  isVideo: false,
  link: null,
});
const titles = (q: Q.QueueState) => q.items.map((i) => i.track.title);
const tracks = [1, 2, 3, 4, 5].map(track);
// Deterministic "random" that reverses order under Fisher-Yates.
const fixed = () => 0;

describe("queue", () => {
  it("plays a list from a start index", () => {
    const q = Q.playNow(Q.emptyQueue, tracks, 2);
    expect(q.index).toBe(2);
    expect(Q.current(q)?.track.title).toBe("Song 3");
    expect(Q.upcoming(q).map((i) => i.track.title)).toEqual(["Song 4", "Song 5"]);
  });

  it("inserts play-next right after the current song", () => {
    const q = Q.playNext(Q.playNow(Q.emptyQueue, tracks, 0), [track(9)]);
    expect(titles(q).slice(0, 3)).toEqual(["Song 1", "Song 9", "Song 2"]);
  });

  it("adds to the end, and starts an empty queue at the first added song", () => {
    const q = Q.addToQueue(Q.emptyQueue, [track(1), track(2)]);
    expect(q.index).toBe(0);
    expect(titles(Q.addToQueue(q, [track(3)]))).toEqual(["Song 1", "Song 2", "Song 3"]);
    expect(Q.playNext(Q.emptyQueue, [track(7)]).index).toBe(0);
  });

  it("keeps the current song when removing or moving others", () => {
    let q = Q.playNow(Q.emptyQueue, tracks, 2);
    q = Q.remove(q, q.items[0].uid);
    expect(Q.current(q)?.track.title).toBe("Song 3");
    q = Q.move(q, 1, 3);
    expect(Q.current(q)?.track.title).toBe("Song 3");
    expect(titles(q)).toEqual(["Song 2", "Song 4", "Song 5", "Song 3"]);
  });

  it("removing the current song moves to the one after it", () => {
    let q = Q.playNow(Q.emptyQueue, tracks, 1);
    q = Q.remove(q, q.items[1].uid);
    expect(Q.current(q)?.track.title).toBe("Song 3");
  });

  it("clears everything except the current song", () => {
    const q = Q.clearUpcoming(Q.playNow(Q.emptyQueue, tracks, 3));
    expect(titles(q)).toEqual(["Song 4"]);
    expect(q.index).toBe(0);
    expect(Q.clearUpcoming(Q.emptyQueue)).toEqual(Q.emptyQueue);
  });

  it("follows repeat modes", () => {
    let q = Q.playNow(Q.emptyQueue, tracks, 4);
    expect(Q.nextIndex(q, true)).toBeNull();
    q = { ...q, repeat: "all" };
    expect(Q.nextIndex(q, true)).toBe(0);
    q = { ...q, repeat: "one" };
    expect(Q.nextIndex(q, true)).toBe(4);
    expect(Q.nextIndex(q, false)).toBe(0);
    expect(Q.cycleRepeat("off")).toBe("all");
    expect(Q.cycleRepeat("one")).toBe("off");
  });

  it("shuffles only upcoming songs and restores order afterwards", () => {
    let q = Q.playNow(Q.emptyQueue, tracks, 1);
    q = Q.setShuffle(q, true, fixed);
    expect(Q.current(q)?.track.title).toBe("Song 2");
    expect(titles(q).slice(0, 2)).toEqual(["Song 1", "Song 2"]);
    expect(new Set(titles(q))).toEqual(new Set(titles(Q.playNow(Q.emptyQueue, tracks))));
    q = Q.addToQueue(q, [track(6)]);
    q = Q.setShuffle(q, false);
    expect(titles(q)).toEqual(["Song 1", "Song 2", "Song 3", "Song 4", "Song 5", "Song 6"]);
    expect(Q.current(q)?.track.title).toBe("Song 2");
  });

  it("starts from the chosen song when playing with shuffle on", () => {
    const q = Q.playNow({ ...Q.emptyQueue, shuffle: true }, tracks, 3, fixed);
    expect(Q.current(q)?.track.title).toBe("Song 4");
    expect(q.index).toBe(0);
    expect(q.items).toHaveLength(5);
  });

  it("rejects malformed saved queues", () => {
    expect(Q.restore(null)).toBeNull();
    expect(Q.restore({ items: "x" })).toBeNull();
    const ok = Q.restore({ items: [{ uid: "a", track: track(1) }, { bad: true }], index: 7, repeat: "nope" });
    expect(ok?.items).toHaveLength(1);
    expect(ok?.index).toBe(0);
    expect(ok?.repeat).toBe("off");
  });
});

describe("hidden songs", () => {
  const hide = (title: string) => (i: Q.QueueItem) => i.track.title === title;

  it("are passed over when the queue moves on by itself", () => {
    const q = Q.playNow(Q.emptyQueue, tracks, 0);
    expect(Q.nextIndex(q, true, hide("Song 2"))).toBe(2);
  });

  it("still play when chosen by hand", () => {
    const q = Q.playNow(Q.emptyQueue, tracks, 0);
    expect(Q.nextIndex(q, false, hide("Song 2"))).toBe(1);
  });

  it("end playback when only hidden songs are left", () => {
    const q = Q.playNow(Q.emptyQueue, tracks, 3);
    expect(Q.nextIndex(q, true, hide("Song 5"))).toBe(null);
  });
});
