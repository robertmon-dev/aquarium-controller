import { describe, expect, it } from "vitest";
import type { HistorySeries } from "@/history/csv";
import { axisTickFormat, latestValue, plotSeries } from "./chart-series";
import { REFERENCE, SERIES } from "./chart-tokens";

function series(field: string, points: [number, number][] = []): HistorySeries {
  return {
    measurement: "aquarium_temperature",
    field,
    kind: "number",
    points: points.map(([time, value]) => ({ time, value })),
  };
}

describe("plotSeries", () => {
  it("assigns the validated slots in order", () => {
    const plotted = plotSeries(["tank_c", "sump_c"]);

    expect(plotted.map((one) => one.color)).toEqual([SERIES[0], SERIES[1]]);
  });

  it("gives a reference line the muted ink and does not spend a slot on it", () => {
    const plotted = plotSeries(["target_c", "tank_c", "sump_c"], "target_c");

    expect(plotted[0]).toMatchObject({ color: REFERENCE, isReference: true });
    expect(plotted[1]?.color).toBe(SERIES[0]);
    expect(plotted[2]?.color).toBe(SERIES[1]);
  });
});

describe("latestValue", () => {
  it("takes the newest sample", () => {
    expect(
      latestValue(
        series("tank_c", [
          [10, 26.1],
          [20, 26.4],
        ]),
      ),
    ).toBe(26.4);
  });

  it("is null for a series with no samples", () => {
    expect(latestValue(series("tank_c"))).toBeNull();
  });
});

describe("axisTickFormat", () => {
  const at = (hour: number, minute = 0) =>
    Math.floor(new Date(2026, 10, 8, hour, minute).getTime() / 1000);

  it("uses HH:mm for a day or less", () => {
    expect(axisTickFormat(3600)(at(14, 5))).toBe("14:05");
    expect(axisTickFormat(24 * 3600)(at(22))).toBe("22:00");
  });

  it("adds the date once a week is in view", () => {
    expect(axisTickFormat(7 * 24 * 3600)(at(18))).toBe("08.11 18:00");
  });

  it("drops the clock for a month", () => {
    expect(axisTickFormat(30 * 24 * 3600)(at(18))).toBe("08.11");
  });
});
