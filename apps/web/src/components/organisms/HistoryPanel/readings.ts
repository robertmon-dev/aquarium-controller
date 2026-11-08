import type { SeriesKind } from "@/history/csv";

export const NO_SAMPLE = "—";

export function formatSample(
  value: number | null | undefined,
  kind: SeriesKind,
  unit = "",
): string {
  if (value === null || value === undefined) return NO_SAMPLE;
  if (kind === "boolean") return value === 1 ? "on" : "off";

  const digits = Number.isInteger(value) ? 0 : 2;
  return unit ? `${value.toFixed(digits)} ${unit}` : value.toFixed(digits);
}
