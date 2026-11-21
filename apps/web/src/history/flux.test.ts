import { describe, expect, it } from "vitest";
import { chartFluxRequest, fluxQuery } from "./flux";
import { CHARTS } from "./series";

const base = {
  bucket: "aquarium",
  measurement: "aquarium_temperature",
  fields: ["tank_c", "sump_c"],
  start: "2026-11-01T00:00:00Z",
  stop: "2026-11-01T01:00:00Z",
};

const DAY_S = 24 * 3600;

function chart(id: string) {
  const found = CHARTS.find((one) => one.id === id);
  if (!found) throw new Error(`no chart ${id}`);
  return found;
}

const sevenDays = {
  start: "2026-11-14T00:00:00Z",
  stop: "2026-11-21T00:00:00Z",
  seconds: 7 * DAY_S,
};

describe("chartFluxRequest", () => {
  it("keeps the last water change phase per window instead of inventing an average one", () => {
    const query = fluxQuery(
      chartFluxRequest(chart("waterChangePhase"), sevenDays, false),
    );

    expect(query).toContain("every: 121s, fn: last");
    expect(query).not.toContain("fn: mean");
  });

  it("still averages continuous series", () => {
    const query = fluxQuery(
      chartFluxRequest(chart("temperature"), sevenDays, false),
    );

    expect(query).toContain("every: 121s, fn: mean");
    expect(query).toContain('set: ["tank_c", "sump_c", "target_tank_c"]');
  });

  it("aggregates every state strip with last", () => {
    for (const one of CHARTS.filter((spec) => spec.kind === "strip")) {
      expect(fluxQuery(chartFluxRequest(one, sevenDays, false))).toContain(
        "fn: last",
      );
    }
  });

  it("asks for raw points when forced", () => {
    const query = fluxQuery(
      chartFluxRequest(chart("waterChangePhase"), sevenDays, true),
    );

    expect(query).not.toContain("aggregateWindow");
  });
});

describe("fluxQuery", () => {
  it("filters by bucket, range, measurement and fields", () => {
    const q = fluxQuery({
      ...base,
      resolution: { mode: "raw" },
      aggregate: "mean",
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
      aggregate: "mean",
    });

    expect(q).not.toContain("aggregateWindow");
  });

  it("aggregates numeric series with mean", () => {
    const q = fluxQuery({
      ...base,
      resolution: { mode: "aggregated", everySeconds: 120 },
      aggregate: "mean",
    });

    expect(q).toContain("aggregateWindow(every: 120s, fn: mean");
  });

  it("aggregates with last when asked, because mean rejects booleans", () => {
    const q = fluxQuery({
      ...base,
      resolution: { mode: "aggregated", everySeconds: 120 },
      aggregate: "last",
    });

    expect(q).toContain("aggregateWindow(every: 120s, fn: last");
    expect(q).not.toContain("fn: mean");
  });

  it("never creates empty windows, so gaps stay gaps", () => {
    const q = fluxQuery({
      ...base,
      resolution: { mode: "aggregated", everySeconds: 60 },
      aggregate: "mean",
    });

    expect(q).toContain("createEmpty: false");
  });

  it("quotes nothing a caller could inject through a field name", () => {
    const q = fluxQuery({
      ...base,
      fields: ['tank_c"] |> drop(columns: ["x'],
      resolution: { mode: "raw" },
      aggregate: "mean",
    });

    expect(q).not.toContain("drop(columns");
  });
});
