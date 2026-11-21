import { resolutionFor, type Resolution } from "./resolution";
import { aggregateOf, type Aggregate, type ChartSpec } from "./series";

export const BUCKET = "aquarium";

export interface HistoryWindow {
  start: string;
  stop: string;
  seconds: number;
}

export interface FluxRequest {
  bucket: string;
  measurement: string;
  fields: string[];
  start: string;
  stop: string;
  resolution: Resolution;
  aggregate: Aggregate;
}

const SAFE = /^[A-Za-z0-9_]+$/;

function safeIdentifiers(values: string[]): string[] {
  return values.filter((value) => SAFE.test(value));
}

export function chartFluxRequest(
  chart: ChartSpec,
  window: HistoryWindow,
  forceRaw: boolean,
): FluxRequest {
  return {
    bucket: BUCKET,
    measurement: chart.measurement,
    fields: chart.reference ? [...chart.fields, chart.reference] : chart.fields,
    start: window.start,
    stop: window.stop,
    resolution: resolutionFor(window.seconds, forceRaw),
    aggregate: aggregateOf(chart),
  };
}

export function fluxQuery(request: FluxRequest): string {
  const fields = safeIdentifiers(request.fields);
  const set = fields.map((field) => `"${field}"`).join(", ");

  const lines = [
    `from(bucket: "${request.bucket}")`,
    `  |> range(start: ${request.start}, stop: ${request.stop})`,
    `  |> filter(fn: (r) => r._measurement == "${request.measurement}")`,
    `  |> filter(fn: (r) => contains(value: r._field, set: [${set}]))`,
  ];

  if (request.resolution.mode === "aggregated") {
    lines.push(
      `  |> aggregateWindow(every: ${request.resolution.everySeconds}s, fn: ${request.aggregate}, createEmpty: false)`,
    );
  }

  return lines.join("\n");
}
