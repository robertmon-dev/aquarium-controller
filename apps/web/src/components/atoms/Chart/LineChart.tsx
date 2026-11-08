import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import type { HistorySeries } from "@/history/csv";
import { alignTimes, valuesAt } from "./chart-data";
import {
  GRID,
  LINE_WIDTH,
  POINT_SIZE,
  REFERENCE,
  SERIES,
  SURFACE,
  TEXT_MUTED,
} from "./chart-tokens";
import { useUplot, type ChartSpec } from "./use-uplot";
import "./Chart.scss";

const SYNC_KEY = "aquarium-history";

export interface LineChartProps {
  title: string;
  unit: string;
  series: HistorySeries[];
  referenceField?: string;
  height?: number;
}

function seriesOption(
  one: HistorySeries,
  stroke: string,
  isReference: boolean,
): uPlot.Series {
  return {
    label: one.field,
    stroke,
    width: isReference ? 1 : LINE_WIDTH,
    ...(isReference ? { dash: [4, 4] } : {}),
    points: {
      show: !isReference,
      size: POINT_SIZE,
      stroke: SURFACE,
      width: 2,
      fill: stroke,
    },
  };
}

function timeAxis(): uPlot.Axis {
  return {
    stroke: TEXT_MUTED,
    grid: { stroke: GRID, width: 1 },
    ticks: { stroke: GRID },
  };
}

export function LineChart({
  title,
  unit,
  series,
  referenceField,
  height = 160,
}: LineChartProps) {
  const host = useUplot(
    (width): ChartSpec | null => {
      if (series.length === 0) return null;

      const times = alignTimes(series);
      let slot = 0;

      return {
        options: {
          title,
          width,
          height,
          cursor: { sync: { key: SYNC_KEY }, points: { size: POINT_SIZE } },
          legend: { show: series.length > 1 },
          scales: { x: { time: true } },
          axes: [timeAxis(), { ...timeAxis(), label: unit }],
          series: [
            {},
            ...series.map((one) => {
              const isReference = one.field === referenceField;
              const stroke = isReference
                ? REFERENCE
                : (SERIES[slot++ % SERIES.length] ?? REFERENCE);

              return seriesOption(one, stroke, isReference);
            }),
          ],
        },
        data: [
          times,
          ...series.map((one) => valuesAt(one, times)),
        ] as uPlot.AlignedData,
      };
    },
    [title, unit, series, referenceField, height],
  );

  return <div className="chart" ref={host} />;
}
