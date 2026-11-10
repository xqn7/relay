import { describe, expect, it } from "vitest";
import { BUILT_IN, preampDb, presetGains, semitoneRatio, validEq, validUserPresets } from "./eq";

describe("equalizer", () => {
  it("lowers the level by the biggest boost only", () => {
    expect(preampDb([6, 2, -3, 0, 0, 0, 0, 0, 0, 0])).toBe(-6);
    expect(preampDb(BUILT_IN.bassReducer)).toBe(-0);
  });

  it("finds built-in and saved presets, not deleted ones", () => {
    const user = [{ name: "Car", gains: [1, 1, 1, 1, 1, 1, 1, 1, 1, 1] }];
    expect(presetGains("rock", user)).toEqual(BUILT_IN.rock);
    expect(presetGains("user:Car", user)).toEqual(user[0].gains);
    expect(presetGains("user:Gone", user)).toBeNull();
    expect(presetGains("custom", user)).toBeNull();
  });

  it("rejects malformed saved settings", () => {
    expect(validEq({ on: true, preset: "flat", gains: BUILT_IN.flat })).toBe(true);
    expect(validEq({ on: true, preset: "flat", gains: [0, 0] })).toBe(false);
    expect(validEq({ on: true, preset: "flat", gains: [99, 0, 0, 0, 0, 0, 0, 0, 0, 0] })).toBe(false);
    expect(validUserPresets([{ name: "", gains: BUILT_IN.flat }])).toBe(false);
  });

  it("turns semitones into a rate", () => {
    expect(semitoneRatio(12)).toBeCloseTo(2);
    expect(semitoneRatio(-12)).toBeCloseTo(0.5);
    expect(semitoneRatio(0)).toBe(1);
  });
});
