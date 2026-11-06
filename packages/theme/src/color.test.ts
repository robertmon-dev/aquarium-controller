import { describe, expect, it } from "vitest";
import { withAlpha } from "./color";

describe("withAlpha", () => {
  it("turns a six digit hex into rgba", () => {
    expect(withAlpha("#7aa2f7", 0.15)).toBe("rgba(122, 162, 247, 0.15)");
  });

  it("expands a three digit hex", () => {
    expect(withAlpha("#abc", 1)).toBe("rgba(170, 187, 204, 1)");
  });

  it("accepts a hex without the hash", () => {
    expect(withAlpha("f7768e", 0.2)).toBe("rgba(247, 118, 142, 0.2)");
  });

  it("clamps an alpha outside the unit range", () => {
    expect(withAlpha("#ffffff", 1.4)).toBe("rgba(255, 255, 255, 1)");
    expect(withAlpha("#ffffff", -0.2)).toBe("rgba(255, 255, 255, 0)");
  });

  it("rejects a colour it cannot parse, so the theme fails at build", () => {
    expect(() => withAlpha("rgb(1,2,3)", 0.5)).toThrow();
    expect(() => withAlpha("#12345", 0.5)).toThrow();
  });
});
