import { describe, expect, it } from "vitest";
import {
  appendSample,
  probeAge,
  probeStability,
  STABILITY_WINDOW,
  tdsFactorFromReference,
} from "./probe-calibration";

describe("probeStability", () => {
  it("reports an empty window before any reading arrives", () => {
    expect(probeStability([])).toBe("empty");
  });

  it("keeps settling until the window is full", () => {
    expect(probeStability([1.5, 1.5, 1.5])).toBe("settling");
  });

  it("calls a tight window stable", () => {
    expect(probeStability([1.5, 1.5005, 1.501, 1.5005, 1.5])).toBe("stable");
  });

  it("calls a drifting window unstable, because a captured point poisons every later reading", () => {
    expect(probeStability([1.5, 1.52, 1.54, 1.56, 1.58])).toBe("settling");
  });

  it("judges only the most recent window, so an early jump stops mattering", () => {
    const samples = [1.0, 1.4, 1.5, 1.5, 1.5, 1.5, 1.5];

    expect(samples.length).toBeGreaterThan(STABILITY_WINDOW);
    expect(probeStability(samples)).toBe("stable");
  });

  it("ignores non-finite samples rather than treating them as a reading", () => {
    expect(probeStability([Number.NaN, Number.NaN])).toBe("empty");
  });
});

describe("tdsFactorFromReference", () => {
  it("scales the current factor by the ratio of reference to reported", () => {
    const factor = tdsFactorFromReference({
      reportedPpm: 400,
      currentFactor: 1,
      referencePpm: 500,
    });

    expect(factor).toBeCloseTo(1.25, 5);
  });

  it("accounts for a factor already in effect, which a naive ratio would discard", () => {
    const factor = tdsFactorFromReference({
      reportedPpm: 500,
      currentFactor: 1.25,
      referencePpm: 500,
    });

    expect(factor).toBeCloseTo(1.25, 5);
  });

  it("refuses to divide by a missing reading", () => {
    expect(
      tdsFactorFromReference({
        reportedPpm: 0,
        currentFactor: 1,
        referencePpm: 500,
      }),
    ).toBeNull();
  });

  it("refuses a non-positive reference", () => {
    expect(
      tdsFactorFromReference({
        reportedPpm: 400,
        currentFactor: 1,
        referencePpm: 0,
      }),
    ).toBeNull();
  });

  it("refuses non-finite input", () => {
    expect(
      tdsFactorFromReference({
        reportedPpm: Number.NaN,
        currentFactor: 1,
        referencePpm: 500,
      }),
    ).toBeNull();
    expect(
      tdsFactorFromReference({
        reportedPpm: 400,
        currentFactor: Number.POSITIVE_INFINITY,
        referencePpm: 500,
      }),
    ).toBeNull();
  });
});

describe("probeAge", () => {
  it("reports how old the sample is", () => {
    expect(probeAge(1_750_000_000, 1_750_000_012)).toBe(12);
  });

  it("reports nothing for a device that has never sampled", () => {
    expect(probeAge(0, 1_750_000_000)).toBeNull();
  });

  it("clamps a sample stamped in the future to zero rather than showing a negative age", () => {
    expect(probeAge(1_750_000_010, 1_750_000_000)).toBe(0);
  });

  it("reports nothing for non-finite input", () => {
    expect(probeAge(Number.NaN, 1_750_000_000)).toBeNull();
  });
});

describe("appendSample", () => {
  it("appends a reading", () => {
    expect(appendSample([1.5], 1.51)).toEqual([1.5, 1.51]);
  });

  it("drops the oldest reading once the window is full", () => {
    const full = [1, 2, 3, 4, 5];

    expect(appendSample(full, 6)).toEqual([2, 3, 4, 5, 6]);
    expect(appendSample(full, 6)).toHaveLength(STABILITY_WINDOW);
  });

  it("keeps the window unchanged when the probe reports nothing", () => {
    expect(appendSample([1.5], null)).toEqual([1.5]);
  });

  it("keeps the window unchanged for a non-finite reading", () => {
    expect(appendSample([1.5], Number.NaN)).toEqual([1.5]);
  });

  it("does not mutate the window it was given", () => {
    const original = [1.5];

    appendSample(original, 1.6);

    expect(original).toEqual([1.5]);
  });
});
