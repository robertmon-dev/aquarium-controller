import { describe, expect, it } from "vitest";
import { formatSample, NO_SAMPLE } from "./readings";

describe("formatSample", () => {
  it("marks a missing sample rather than printing zero", () => {
    expect(formatSample(null, "number")).toBe(NO_SAMPLE);
    expect(formatSample(undefined, "number")).toBe(NO_SAMPLE);
  });

  it("reads a boolean as a state, not a number", () => {
    expect(formatSample(1, "boolean")).toBe("on");
    expect(formatSample(0, "boolean")).toBe("off");
  });

  it("keeps two decimals for a measurement and none for a whole number", () => {
    expect(formatSample(26.456, "number", "°C")).toBe("26.46 °C");
    expect(formatSample(60, "number", "%")).toBe("60 %");
  });

  it("omits the unit when there is none", () => {
    expect(formatSample(0.94, "number")).toBe("0.94");
  });

  it("does not swallow a genuine zero", () => {
    expect(formatSample(0, "number", "%")).toBe("0 %");
  });
});
