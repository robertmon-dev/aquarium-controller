import { describe, expect, it } from "vitest";
import type { HistorySeries } from "@/history/csv";
import { alignTimes, stackedRows, valuesAt } from "./chart-data";

function series(
  field: string,
  points: [number, number][],
  kind: HistorySeries["kind"] = "number",
): HistorySeries {
  return {
    measurement: "aquarium_temperature",
    field,
    kind,
    points: points.map(([time, value]) => ({ time, value })),
  };
}

describe("alignTimes", () => {
  it("unions the timestamps and sorts them", () => {
    const times = alignTimes([
      series("a", [
        [30, 1],
        [10, 2],
      ]),
      series("b", [
        [20, 3],
        [10, 4],
      ]),
    ]);

    expect(times).toEqual([10, 20, 30]);
  });

  it("has nothing to align for no series", () => {
    expect(alignTimes([])).toEqual([]);
  });
});

describe("valuesAt", () => {
  it("leaves a gap null rather than carrying the last value forward", () => {
    const values = valuesAt(
      series("a", [
        [10, 1],
        [30, 3],
      ]),
      [10, 20, 30],
    );

    expect(values).toEqual([1, null, 3]);
  });
});

describe("stackedRows", () => {
  it("lifts each series onto its own band, two units apart", () => {
    const rows = stackedRows(
      [
        series("heater", [[10, 1]], "boolean"),
        series("relay", [[10, 1]], "boolean"),
        series("flow_ok", [[10, 0]], "boolean"),
      ],
      [10],
    );

    expect(rows).toEqual([[1], [3], [4]]);
  });

  it("keeps a missing sample null so the strip breaks instead of dropping to the band floor", () => {
    const rows = stackedRows(
      [series("heater", [[10, 1]], "boolean")],
      [10, 20],
    );

    expect(rows).toEqual([[1, null]]);
  });
});
