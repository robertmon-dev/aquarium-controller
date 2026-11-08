import type { HistorySeries, SeriesKind } from "@/history/csv";

export interface TableColumn {
  field: string;
  kind: SeriesKind;
}

export interface TableRow {
  time: number;
  cells: (number | undefined)[];
}

export interface HistoryTableModel {
  columns: TableColumn[];
  rows: TableRow[];
}

/**
 * Newest first and capped: the table is for reading values off a window, not
 * for scrolling tens of thousands of samples. The chart carries the shape.
 */
export function historyTableModel(
  series: HistorySeries[],
  limit = 200,
): HistoryTableModel {
  const times = [
    ...new Set(series.flatMap((one) => one.points.map((p) => p.time))),
  ]
    .sort((a, b) => b - a)
    .slice(0, limit);

  const lookups = series.map(
    (one) => new Map(one.points.map((p) => [p.time, p.value])),
  );

  return {
    columns: series.map((one) => ({ field: one.field, kind: one.kind })),
    rows: times.map((time) => ({
      time,
      cells: lookups.map((lookup) => lookup.get(time)),
    })),
  };
}
