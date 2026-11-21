import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import type { HistorySeries } from "@/history/csv";
import { alignTimes, valuesAt } from "./chart-data";
import {
  axisTickFormat,
  colorOf,
  isReference,
  type PlottedSeries,
} from "./chart-series";
import {
  GRID,
  LINE_WIDTH,
  POINT_SIZE,
  SURFACE,
  TEXT_MUTED,
} from "./chart-tokens";
import { useUplot, type ChartSpec } from "./use-uplot";
import "./Chart.scss";

const SYNC_KEY = "aquarium-history";
const DEFAULT_HEIGHT = 300;

export interface LineChartProps {
  unit: string;
  series: HistorySeries[];
  range: [number, number];
  plotted: PlottedSeries[];
  height?: number;
  stepped?: boolean;
  levels?: readonly string[] | undefined;
}

export function LineChart({
  unit,
  series,
  range,
  plotted,
  height = DEFAULT_HEIGHT,
  stepped = false,
  levels,
}: LineChartProps) {
  const host = useUplot(
    (width): ChartSpec | null => {
      if (series.length === 0) return null;

      const times = alignTimes(series);
      const formatTick = axisTickFormat(range[1] - range[0]);
      const steppedPath = uPlot.paths.stepped?.({ align: 1 });

      return {
        options: {
          width,
          height,
          cursor: { sync: { key: SYNC_KEY }, points: { size: POINT_SIZE } },
          legend: { show: false },
          scales: {
            x: { time: true, range: [range[0], range[1]] },
            ...(levels ? { y: { range: [0, levels.length - 1] } } : {}),
          },
          axes: [
            {
              stroke: TEXT_MUTED,
              grid: { stroke: GRID, width: 1 },
              ticks: { stroke: GRID },
              values: (_, splits) => splits.map(formatTick),
            },
            {
              stroke: TEXT_MUTED,
              grid: { stroke: GRID, width: 1 },
              ticks: { stroke: GRID },
              label: unit,
              ...(levels
                ? {
                    size: 90,
                    splits: () => levels.map((_, index) => index),
                    values: (_: uPlot, splits: number[]) =>
                      splits.map((split) => levels[split] ?? ""),
                  }
                : {}),
            },
          ],
          series: [
            {},
            ...series.map((one) => {
              const color = colorOf(plotted, one.field);
              const reference = isReference(plotted, one.field);

              return {
                label: one.field,
                stroke: color,
                width: reference ? 1 : LINE_WIDTH,
                ...(stepped && steppedPath ? { paths: steppedPath } : {}),
                ...(reference ? { dash: [4, 4] } : {}),
                points: {
                  show: !reference,
                  size: POINT_SIZE,
                  stroke: SURFACE,
                  width: 2,
                  fill: color,
                },
              };
            }),
          ],
        },
        data: [
          times,
          ...series.map((one) => valuesAt(one, times)),
        ] as uPlot.AlignedData,
      };
    },
    [unit, series, range, plotted, height, stepped, levels],
  );

  return <div className="chart" ref={host} />;
}
