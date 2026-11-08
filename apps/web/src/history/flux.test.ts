import { describe, expect, it } from "vitest";
import { fluxQuery } from "./flux";

const base = {
  bucket: "aquarium",
  measurement: "aquarium_temperature",
  fields: ["tank_c", "sump_c"],
  start: "2026-11-01T00:00:00Z",
  stop: "2026-11-01T01:00:00Z",
};

describe("fluxQuery", () => {
  it("filters by bucket, range, measurement and fields", () => {
    const q = fluxQuery({
      ...base,
      resolution: { mode: "raw" },
      kind: "line",
    });

    expect(q).toContain('from(bucket: "aquarium")');
    expect(q).toContain("start: 2026-11-01T00:00:00Z");
    expect(q).toContain("stop: 2026-11-01T01:00:00Z");
    expect(q).toContain('r._measurement == "aquarium_temperature"');
    expect(q).toContain('set: ["tank_c", "sump_c"]');
  });

  it("adds no aggregateWindow when raw", () => {
    const q = fluxQuery({
      ...base,
      resolution: { mode: "raw" },
      kind: "line",
    });

    expect(q).not.toContain("aggregateWindow");
  });

  it("aggregates numeric series with mean", () => {
    const q = fluxQuery({
      ...base,
      resolution: { mode: "aggregated", everySeconds: 120 },
      kind: "line",
    });

    expect(q).toContain("aggregateWindow(every: 120s, fn: mean");
  });

  it("aggregates state series with last, because mean rejects booleans", () => {
    const q = fluxQuery({
      ...base,
      resolution: { mode: "aggregated", everySeconds: 120 },
      kind: "strip",
    });

    expect(q).toContain("aggregateWindow(every: 120s, fn: last");
    expect(q).not.toContain("fn: mean");
  });

  it("never creates empty windows, so gaps stay gaps", () => {
    const q = fluxQuery({
      ...base,
      resolution: { mode: "aggregated", everySeconds: 60 },
      kind: "line",
    });

    expect(q).toContain("createEmpty: false");
  });

  it("quotes nothing a caller could inject through a field name", () => {
    const q = fluxQuery({
      ...base,
      fields: ['tank_c"] |> drop(columns: ["x'],
      resolution: { mode: "raw" },
      kind: "line",
    });

    expect(q).not.toContain("drop(columns");
  });
});
