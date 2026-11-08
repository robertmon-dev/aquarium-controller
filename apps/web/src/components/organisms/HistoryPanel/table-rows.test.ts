import { describe, expect, it } from "vitest";
import type { HistorySeries } from "@/history/csv";
import { historyTableModel } from "./table-rows";

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

describe("historyTableModel", () => {
  it("lists one column per series, keeping its kind", () => {
    const model = historyTableModel([
      series("tank_c", [[10, 26.5]]),
      series("heater", [[10, 1]], "boolean"),
    ]);

    expect(model.columns).toEqual([
      { field: "tank_c", kind: "number" },
      { field: "heater", kind: "boolean" },
    ]);
  });

  it("orders rows newest first", () => {
    const model = historyTableModel([
      series("tank_c", [
        [10, 1],
        [30, 3],
        [20, 2],
      ]),
    ]);

    expect(model.rows.map((row) => row.time)).toEqual([30, 20, 10]);
  });

  it("caps the rows so a day of samples does not land in the DOM", () => {
    const points: [number, number][] = Array.from({ length: 500 }, (_, i) => [
      i,
      i,
    ]);

    expect(historyTableModel([series("tank_c", points)], 200).rows).toHaveLength(
      200,
    );
  });

  it("leaves a missing sample undefined rather than zero", () => {
    const model = historyTableModel([
      series("tank_c", [[10, 26.5]]),
      series("sump_c", [[20, 26]]),
    ]);

    const newest = model.rows[0];
    const oldest = model.rows[1];

    expect(newest?.cells).toEqual([undefined, 26]);
    expect(oldest?.cells).toEqual([26.5, undefined]);
  });

  it("has nothing to show for no series", () => {
    expect(historyTableModel([])).toEqual({ columns: [], rows: [] });
  });
});
