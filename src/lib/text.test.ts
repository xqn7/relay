import { describe, expect, it } from "vitest";
import { fold, matchesQuery } from "./text";

describe("fold", () => {
  it("matches the backend's folding", () => {
    expect(fold("ФРУКТОВЫЙ")).toBe("fruktovyy");
    expect(fold("Beyoncé Señorita Ærø")).toBe("beyonce senorita aero");
  });
  it("searches case- and accent-insensitively", () => {
    expect(matchesQuery("Café del Mar", fold("cafe"))).toBe(true);
    expect(matchesQuery("Anything", "")).toBe(true);
    expect(matchesQuery("Song", fold("other"))).toBe(false);
  });
});
