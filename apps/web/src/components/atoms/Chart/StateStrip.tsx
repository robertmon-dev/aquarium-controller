import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import type { HistorySeries } from "@/history/csv";
import { alignTimes, stackedRows } from "./chart-data";
import { GRID, SERIES, TEXT_MUTED } from "./chart-tokens";
import { useUplot, type ChartSpec } from "./use-uplot";
import "./Chart.scss";

const SYNC_KEY = "aquarium-history";
const ROW_HEIGHT = 34;

export interface StateStripProps {
  title: string;
  series: HistorySeries[];
}

export function StateStrip({ title, series }: StateStripProps) {
  const host = useUplot(
    (width): ChartSpec | null => {
      if (series.length === 0) return null;

      const times = alignTimes(series);
      const stepped = uPlot.paths.stepped?.({ align: 1 });

      return {
        options: {
          title,
          width,
          height: Math.max(ROW_HEIGHT * series.length, ROW_HEIGHT),
          cursor: { sync: { key: SYNC_KEY }, points: { show: false } },
          legend: { show: series.length > 1 },
          scales: {
            x: { time: true },
            y: { range: [-0.5, series.length * 2] },
          },
          axes: [
            { stroke: TEXT_MUTED, grid: { stroke: GRID, width: 1 } },
            { show: false },
          ],
          series: [
            {},
            ...series.map((one, row) => {
              const stroke = SERIES[row % SERIES.length] ?? SERIES[0];

              return {
                label: one.field,
                stroke,
                width: 2,
                fill: `${stroke}1a`,
                ...(stepped ? { paths: stepped } : {}),
                points: { show: false },
              } satisfies uPlot.Series;
            }),
          ],
        },
        data: [times, ...stackedRows(series, times)] as uPlot.AlignedData,
      };
    },
    [title, series],
  );

  return <div className="chart chart--strip" ref={host} />;
}
