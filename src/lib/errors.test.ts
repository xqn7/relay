import { describe, expect, it } from "vitest";
import { explain } from "./errors";

describe("smart errors", () => {
  it("names a known error and says how to fix it", () => {
    const x = explain({ code: "nothing_playable", detail: "timed out after 20 s" });
    expect(x.name).toBe("Can't play this song");
    expect(x.fix).toContain("Fix match");
    expect(x.detail).toBe("timed out after 20 s");
    expect(x.action).toBeNull();
  });

  it("offers the right fix button", () => {
    expect(explain({ code: "timeout", detail: "" }).action).toBe("connection");
    expect(explain({ code: "signin_required", detail: "" }).action).toBe("youtube");
  });

  it("keeps the code of an error it has no words for", () => {
    const x = explain({ code: "something_new", detail: "d" });
    expect(x.name).toBe("Unexpected error");
    expect(x.code).toBe("something_new");
    expect(explain(new Error("boom")).detail).toBe("boom");
  });
});
