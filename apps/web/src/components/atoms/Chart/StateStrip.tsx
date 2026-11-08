import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import type { HistorySeries } from "@/history/csv";
import { alignTimes, stackedRows } from "./chart-data";
import { axisTickFormat, colorOf, type PlottedSeries } from "./chart-series";
import { GRID, TEXT_MUTED } from "./chart-tokens";
import { useUplot, type ChartSpec } from "./use-uplot";
import "./Chart.scss";

const SYNC_KEY = "aquarium-history";
const ROW_HEIGHT = 40;

export interface StateStripProps {
  series: HistorySeries[];
  range: [number, number];
  plotted: PlottedSeries[];
}

export function StateStrip({ series, range, plotted }: StateStripProps) {
  const host = useUplot(
    (width): ChartSpec | null => {
      if (series.length === 0) return null;

      const times = alignTimes(series);
      const formatTick = axisTickFormat(range[1] - range[0]);
      const stepped = uPlot.paths.stepped?.({ align: 1 });
      const height = Math.max(ROW_HEIGHT * series.length, ROW_HEIGHT * 2);

      return {
        options: {
          width,
          height,
          cursor: { sync: { key: SYNC_KEY }, points: { show: false } },
          legend: { show: false },
          scales: {
            x: { time: true, range: [range[0], range[1]] },
            y: { range: [-0.5, series.length * 2] },
          },
          axes: [
            {
              stroke: TEXT_MUTED,
              grid: { stroke: GRID, width: 1 },
              values: (_, splits) => splits.map(formatTick),
            },
            { show: false },
          ],
          series: [
            {},
            ...series.map((one) => {
              const color = colorOf(plotted, one.field);

              return {
                label: one.field,
                stroke: color,
                width: 2,
                fill: `${color}1a`,
                ...(stepped ? { paths: stepped } : {}),
                points: { show: false },
              };
            }),
          ],
        },
        data: [times, ...stackedRows(series, times)] as uPlot.AlignedData,
      };
    },
    [series, range, plotted],
  );

  return <div className="chart chart--strip" ref={host} />;
}
