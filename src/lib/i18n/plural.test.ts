import { describe, expect, it } from "vitest";
import { pickPlural } from "./plural";

describe("pickPlural", () => {
  it("keeps English and German one/other", () => {
    for (const lang of ["en", "de"]) {
      expect(pickPlural(lang, ["1 song", "{n} songs"], 1)).toBe("1 song");
      expect(pickPlural(lang, ["1 song", "{n} songs"], 0)).toBe("{n} songs");
      expect(pickPlural(lang, ["1 song", "{n} songs"], 7)).toBe("{n} songs");
    }
  });

  it("uses a leading zero form when there is one", () => {
    const forms = ["Never played", "Played once", "Played {n} times"];
    expect(pickPlural("en", forms, 0)).toBe("Never played");
    expect(pickPlural("en", forms, 1)).toBe("Played once");
    expect(pickPlural("en", forms, 3)).toBe("Played {n} times");
  });

  it("gives Polish its three forms", () => {
    const forms = ["1 utwór", "{n} utwory", "{n} utworów"];
    expect(pickPlural("pl", forms, 1)).toBe("1 utwór");
    expect(pickPlural("pl", forms, 2)).toBe("{n} utwory");
    expect(pickPlural("pl", forms, 4)).toBe("{n} utwory");
    expect(pickPlural("pl", forms, 5)).toBe("{n} utworów");
    expect(pickPlural("pl", forms, 12)).toBe("{n} utworów");
    expect(pickPlural("pl", forms, 22)).toBe("{n} utwory");
    expect(pickPlural("pl", ["Nigdy", "Raz", "{n} razy", "{n} razy"], 0)).toBe("Nigdy");
  });

  it("follows French, where zero is singular", () => {
    expect(pickPlural("fr", ["{n} titre", "{n} titres"], 0)).toBe("{n} titre");
    expect(pickPlural("fr", ["{n} titre", "{n} titres"], 2)).toBe("{n} titres");
  });
});
