import { describe, expect, it } from "vitest";
import {
  advanceProbeSamples,
  appendSample,
  canCapturePh,
  canUseTdsReading,
  PROBE_FRESHNESS_SECONDS,
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

  it("refuses a sample stamped in the future for capture", () => {
    const ageSeconds = probeAge(1_750_000_010, 1_750_000_000);
    expect(ageSeconds).toBeNull();
    expect(canCapturePh({ stability: "stable", volts: 1.5, ageSeconds })).toBe(
      false,
    );
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

  it("clears the window when the probe reports nothing", () => {
    expect(appendSample([1.5], null)).toEqual([]);
  });

  it("clears the window for a non-finite reading", () => {
    expect(appendSample([1.5], Number.NaN)).toEqual([]);
  });

  it("does not let readings from before a dropout vouch for the one after it", () => {
    const stable = [1.5, 1.5, 1.5, 1.5, 1.5];
    const afterGap = appendSample(appendSample(stable, null), 1.5005);

    expect(probeStability(afterGap)).toBe("settling");
  });

  it("does not mutate the window it was given", () => {
    const original = [1.5];

    appendSample(original, 1.6);

    expect(original).toEqual([1.5]);
  });
});

describe("advanceProbeSamples", () => {
  const stable = { ph: [1.5, 1.5, 1.5, 1.5, 1.5], tds: [0.9, 0.9, 0.9] };

  it("appends each channel's reading", () => {
    const next = advanceProbeSamples(
      stable,
      { ph: 1.5005, tds: 0.91, sampled_at_epoch_s: 1_750_000_002 },
      1_750_000_000,
    );

    expect(next.ph).toEqual([1.5, 1.5, 1.5, 1.5, 1.5005]);
    expect(next.tds).toEqual([0.9, 0.9, 0.9, 0.91]);
  });

  it("clears only the channel that reported nothing", () => {
    const next = advanceProbeSamples(
      stable,
      { ph: null, tds: 0.91, sampled_at_epoch_s: 1_750_000_002 },
      1_750_000_000,
    );

    expect(next.ph).toEqual([]);
    expect(next.tds).toEqual([0.9, 0.9, 0.9, 0.91]);
  });

  it("starts both windows over after a gap longer than the freshness threshold", () => {
    const next = advanceProbeSamples(
      stable,
      {
        ph: 1.5005,
        tds: 0.91,
        sampled_at_epoch_s: 1_750_000_000 + PROBE_FRESHNESS_SECONDS + 1,
      },
      1_750_000_000,
    );

    expect(next.ph).toEqual([1.5005]);
    expect(next.tds).toEqual([0.91]);
    expect(probeStability(next.ph)).toBe("settling");
  });

  it("restarts both windows when the device clock steps backward", () => {
    const next = advanceProbeSamples(
      stable,
      { ph: 1.5, tds: 0.9, sampled_at_epoch_s: 1000 },
      4600,
    );
    expect(next).toEqual({ ph: [1.5], tds: [0.9] });
    expect(probeStability(next.ph)).toBe("settling");
  });

  it("keeps the windows across a gap at the threshold", () => {
    const next = advanceProbeSamples(
      stable,
      {
        ph: 1.5,
        tds: 0.9,
        sampled_at_epoch_s: 1_750_000_000 + PROBE_FRESHNESS_SECONDS,
      },
      1_750_000_000,
    );

    expect(probeStability(next.ph)).toBe("stable");
  });

  it("appends the first reading when there is no previous stamp", () => {
    const next = advanceProbeSamples(
      { ph: [], tds: [] },
      { ph: 1.5, tds: 0.9, sampled_at_epoch_s: 1_750_000_000 },
      null,
    );

    expect(next).toEqual({ ph: [1.5], tds: [0.9] });
  });
});

describe("canUseTdsReading", () => {
  it("allows a positive reading up to the freshness threshold", () => {
    expect(canUseTdsReading({ reportedPpm: 400, ageSeconds: 2 })).toBe(true);
    expect(
      canUseTdsReading({
        reportedPpm: 400,
        ageSeconds: PROBE_FRESHNESS_SECONDS,
      }),
    ).toBe(true);
  });

  it("refuses a stale, missing or zero reading", () => {
    expect(canUseTdsReading({ reportedPpm: 400, ageSeconds: 60 })).toBe(false);
    expect(canUseTdsReading({ reportedPpm: 400, ageSeconds: null })).toBe(
      false,
    );
    expect(canUseTdsReading({ reportedPpm: null, ageSeconds: 2 })).toBe(false);
    expect(canUseTdsReading({ reportedPpm: 0, ageSeconds: 2 })).toBe(false);
  });
});

describe("canCapturePh", () => {
  it("allows a stable, fresh reading", () => {
    expect(canCapturePh({ stability: "stable", volts: 1.5, ageSeconds: 2 })).toBe(
      true,
    );
  });

  it("refuses a reading older than the freshness threshold", () => {
    expect(
      canCapturePh({ stability: "stable", volts: 1.5, ageSeconds: 60 }),
    ).toBe(false);
    expect(
      canCapturePh({
        stability: "stable",
        volts: 1.5,
        ageSeconds: PROBE_FRESHNESS_SECONDS + 1,
      }),
    ).toBe(false);
  });

  it("allows a reading exactly at the threshold, matching the stale warning", () => {
    expect(
      canCapturePh({
        stability: "stable",
        volts: 1.5,
        ageSeconds: PROBE_FRESHNESS_SECONDS,
      }),
    ).toBe(true);
  });

  it("refuses a reading of unknown age", () => {
    expect(
      canCapturePh({ stability: "stable", volts: 1.5, ageSeconds: null }),
    ).toBe(false);
  });

  it("refuses a settling reading", () => {
    expect(
      canCapturePh({ stability: "settling", volts: 1.5, ageSeconds: 2 }),
    ).toBe(false);
  });

  it("refuses a missing voltage", () => {
    expect(
      canCapturePh({ stability: "stable", volts: null, ageSeconds: 2 }),
    ).toBe(false);
  });
});
