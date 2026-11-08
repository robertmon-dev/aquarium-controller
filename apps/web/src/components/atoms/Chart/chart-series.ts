import type { HistorySeries } from "@/history/csv";
import { REFERENCE, SERIES } from "./chart-tokens";

export interface PlottedSeries {
  field: string;
  color: string;
  isReference: boolean;
}

export function plotSeries(
  fields: readonly string[],
  referenceField?: string,
): PlottedSeries[] {
  let slot = 0;

  return fields.map((field) => {
    const isReference = field === referenceField;

    return {
      field,
      color: isReference
        ? REFERENCE
        : (SERIES[slot++ % SERIES.length] ?? REFERENCE),
      isReference,
    };
  });
}

export function colorOf(plotted: PlottedSeries[], field: string): string {
  return plotted.find((one) => one.field === field)?.color ?? REFERENCE;
}

export function isReference(plotted: PlottedSeries[], field: string): boolean {
  return plotted.find((one) => one.field === field)?.isReference ?? false;
}

export function latestValue(one: HistorySeries): number | null {
  return one.points.at(-1)?.value ?? null;
}

const HOUR = 3600;
const DAY = 24 * HOUR;

function pad(value: number): string {
  return String(value).padStart(2, "0");
}

export function axisTickFormat(
  spanSeconds: number,
): (epochSeconds: number) => string {
  if (spanSeconds <= DAY) {
    return (seconds) => {
      const date = new Date(seconds * 1000);
      return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
    };
  }

  if (spanSeconds <= 7 * DAY) {
    return (seconds) => {
      const date = new Date(seconds * 1000);
      return `${pad(date.getDate())}.${pad(date.getMonth() + 1)} ${pad(date.getHours())}:00`;
    };
  }

  return (seconds) => {
    const date = new Date(seconds * 1000);
    return `${pad(date.getDate())}.${pad(date.getMonth() + 1)}`;
  };
}
