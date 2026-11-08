import type { Resolution } from "./resolution";
import type { ChartKind } from "./series";

export interface FluxRequest {
  bucket: string;
  measurement: string;
  fields: string[];
  start: string;
  stop: string;
  resolution: Resolution;
  kind: ChartKind;
}

const SAFE = /^[A-Za-z0-9_]+$/;

function safeIdentifiers(values: string[]): string[] {
  return values.filter((value) => SAFE.test(value));
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
    const fn = request.kind === "strip" ? "last" : "mean";
    lines.push(
      `  |> aggregateWindow(every: ${request.resolution.everySeconds}s, fn: ${fn}, createEmpty: false)`,
    );
  }

  return lines.join("\n");
}
