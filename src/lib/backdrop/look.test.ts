import { describe, expect, it } from "vitest";
import { defaultLook, lookFilter, readLook } from "./look";

describe("background look", () => {
  it("falls back to defaults for anything malformed", () => {
    expect(readLook(null)).toEqual(defaultLook);
    expect(readLook([1, 2])).toEqual(defaultLook);
    expect(readLook({ blur: "a lot", mix: "yes" })).toEqual(defaultLook);
  });

  it("keeps values inside their ranges", () => {
    const l = readLook({ blur: 400, dim: -1, zoom: 2, x: 10, mix: true });
    expect(l).toEqual({ ...defaultLook, blur: 40, dim: 0, zoom: 2, x: 10, mix: true });
    expect(lookFilter(l)).toBe("blur(40px) brightness(1) saturate(1)");
  });
});
