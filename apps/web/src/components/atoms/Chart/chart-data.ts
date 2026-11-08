import type { HistorySeries } from "@/history/csv";

export function alignTimes(series: HistorySeries[]): number[] {
  const times = new Set<number>();

  for (const one of series) {
    for (const point of one.points) times.add(point.time);
  }

  return [...times].sort((a, b) => a - b);
}

export function valuesAt(
  one: HistorySeries,
  times: number[],
): (number | null)[] {
  const byTime = new Map(one.points.map((point) => [point.time, point.value]));

  return times.map((time) => byTime.get(time) ?? null);
}

export function stackedRows(
  series: HistorySeries[],
  times: number[],
): (number | null)[][] {
  return series.map((one, row) => {
    const byTime = new Map(one.points.map((p) => [p.time, p.value]));

    return times.map((time) => {
      const value = byTime.get(time);
      return value === undefined ? null : row * 2 + value;
    });
  });
}
