import { describe, expect, it } from "vitest";
import { MAX_POINTS, resolutionFor, SAMPLE_INTERVAL_S } from "./resolution";

const HOUR = 3600;

describe("resolutionFor", () => {
  it("keeps an hour raw", () => {
    expect(resolutionFor(HOUR, false)).toEqual({ mode: "raw" });
  });

  it("keeps a day raw, which is why uPlot was chosen", () => {
    expect(resolutionFor(24 * HOUR, false)).toEqual({ mode: "raw" });
  });

  it("aggregates a week, targeting the point budget", () => {
    const result = resolutionFor(7 * 24 * HOUR, false);

    expect(result.mode).toBe("aggregated");
    if (result.mode !== "aggregated") throw new Error("expected aggregation");
    expect((7 * 24 * HOUR) / result.everySeconds).toBeLessThanOrEqual(
      MAX_POINTS,
    );
  });

  it("never windows below the sampling interval", () => {
    const result = resolutionFor(25 * HOUR, false);

    if (result.mode !== "aggregated") throw new Error("expected aggregation");
    expect(result.everySeconds).toBeGreaterThanOrEqual(SAMPLE_INTERVAL_S);
  });

  it("honours an explicit request for raw data on a long range", () => {
    expect(resolutionFor(30 * 24 * HOUR, true)).toEqual({ mode: "raw" });
  });

  it("rounds the window up, so the budget is never exceeded", () => {
    const result = resolutionFor(30 * 24 * HOUR, false);

    if (result.mode !== "aggregated") throw new Error("expected aggregation");
    expect(Number.isInteger(result.everySeconds)).toBe(true);
    expect((30 * 24 * HOUR) / result.everySeconds).toBeLessThanOrEqual(
      MAX_POINTS,
    );
  });
});
