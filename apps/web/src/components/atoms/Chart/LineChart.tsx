import { useEffect, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import type { HistorySeries } from "@/history/csv";
import {
  GRID,
  LINE_WIDTH,
  POINT_SIZE,
  REFERENCE,
  SERIES,
  SURFACE,
  TEXT_MUTED,
} from "./chart-tokens";
import "./Chart.scss";

const SYNC_KEY = "aquarium-history";

export interface LineChartProps {
  title: string;
  unit: string;
  series: HistorySeries[];
  referenceField?: string;
  height?: number;
}

function alignTimes(series: HistorySeries[]): number[] {
  const times = new Set<number>();
  for (const one of series) {
    for (const point of one.points) times.add(point.time);
  }
  return [...times].sort((a, b) => a - b);
}

function valuesAt(one: HistorySeries, times: number[]): (number | null)[] {
  const byTime = new Map(one.points.map((p) => [p.time, p.value]));
  return times.map((time) => byTime.get(time) ?? null);
}

export function LineChart({
  title,
  unit,
  series,
  referenceField,
  height = 160,
}: LineChartProps) {
  const host = useRef<HTMLDivElement>(null);
  const plot = useRef<uPlot | null>(null);

  useEffect(() => {
    const node = host.current;
    if (!node || series.length === 0) return;

    const times = alignTimes(series);
    const data = [times, ...series.map((one) => valuesAt(one, times))];

    let slot = 0;
    const options: uPlot.Options = {
      title,
      width: node.clientWidth || 600,
      height,
      cursor: { sync: { key: SYNC_KEY }, points: { size: POINT_SIZE } },
      legend: { show: series.length > 1 },
      scales: { x: { time: true } },
      axes: [
        {
          stroke: TEXT_MUTED,
          grid: { stroke: GRID, width: 1 },
          ticks: { stroke: GRID },
        },
        {
          stroke: TEXT_MUTED,
          grid: { stroke: GRID, width: 1 },
          ticks: { stroke: GRID },
          label: unit,
        },
      ],
      series: [
        {},
        ...series.map((one) => {
          const isReference = one.field === referenceField;
          const stroke = isReference
            ? REFERENCE
            : (SERIES[slot++ % SERIES.length] ?? REFERENCE);

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
          } satisfies uPlot.Series;
        }),
      ],
    };

    plot.current = new uPlot(options, data as uPlot.AlignedData, node);

    const resize = () =>
      plot.current?.setSize({ width: node.clientWidth, height });
    window.addEventListener("resize", resize);

    return () => {
      window.removeEventListener("resize", resize);
      plot.current?.destroy();
      plot.current = null;
    };
  }, [title, unit, series, referenceField, height]);

  return <div className="chart" ref={host} />;
}
