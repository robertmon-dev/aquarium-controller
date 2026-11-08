import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { parseAnnotatedCsv } from "./csv";

const fixture = readFileSync(
  path.join(import.meta.dirname, "fixtures/annotated.csv"),
  "utf8",
);

describe("parseAnnotatedCsv", () => {
  it("reads every series across the repeating annotation blocks", () => {
    const series = parseAnnotatedCsv(fixture);
    const fields = series.map((s) => s.field).sort();

    expect(fields).toEqual([
      "healthy",
      "heater",
      "sump_c",
      "tank_c",
      "target_c",
    ]);
  });

  it("types numeric values as numbers", () => {
    const tank = parseAnnotatedCsv(fixture).find((s) => s.field === "tank_c");

    expect(tank?.kind).toBe("number");
    expect(tank?.points[0]?.value).toBe(26.5);
  });

  it("maps booleans to 0 and 1, so uPlot can plot them", () => {
    const heater = parseAnnotatedCsv(fixture).find((s) => s.field === "heater");
    const healthy = parseAnnotatedCsv(fixture).find(
      (s) => s.field === "healthy",
    );

    expect(heater?.kind).toBe("boolean");
    expect(heater?.points[0]?.value).toBe(0);
    expect(healthy?.points[0]?.value).toBe(1);
  });

  it("parses timestamps into epoch seconds", () => {
    const tank = parseAnnotatedCsv(fixture).find((s) => s.field === "tank_c");

    expect(tank?.points[0]?.time).toBe(
      Math.floor(Date.parse("2026-11-08T12:25:29Z") / 1000),
    );
  });

  it("strips the carriage returns, so the last column is clean", () => {
    for (const series of parseAnnotatedCsv(fixture)) {
      expect(series.measurement).toBe("aquarium_temperature");
      expect(series.measurement).not.toContain("\r");
    }
  });

  it("returns nothing for an empty body rather than throwing", () => {
    expect(parseAnnotatedCsv("")).toEqual([]);
    expect(parseAnnotatedCsv("\r\n\r\n")).toEqual([]);
  });

  it("skips a row whose value is empty", () => {
    const body = [
      "#datatype,string,long,dateTime:RFC3339,double,string,string",
      "#group,false,false,false,false,true,true",
      "#default,_result,,,,,",
      ",result,table,_time,_value,_field,_measurement",
      ",,0,2026-11-08T12:25:29Z,,tank_c,aquarium_temperature",
      ",,0,2026-11-08T12:25:31Z,26.5,tank_c,aquarium_temperature",
    ].join("\r\n");

    const tank = parseAnnotatedCsv(body)[0];

    expect(tank?.points).toHaveLength(1);
    expect(tank?.points[0]?.value).toBe(26.5);
  });
});
