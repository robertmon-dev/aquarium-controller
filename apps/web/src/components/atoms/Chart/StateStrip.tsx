import { useEffect, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import type { HistorySeries } from "@/history/csv";
import { GRID, SERIES, TEXT_MUTED } from "./chart-tokens";
import "./Chart.scss";

const SYNC_KEY = "aquarium-history";
const ROW_HEIGHT = 34;

export interface StateStripProps {
  title: string;
  series: HistorySeries[];
}

export function StateStrip({ title, series }: StateStripProps) {
  const host = useRef<HTMLDivElement>(null);
  const plot = useRef<uPlot | null>(null);

  useEffect(() => {
    const node = host.current;
    if (!node || series.length === 0) return;

    const times = new Set<number>();
    for (const one of series) for (const p of one.points) times.add(p.time);
    const x = [...times].sort((a, b) => a - b);

    const data = [
      x,
      ...series.map((one, row) => {
        const byTime = new Map(one.points.map((p) => [p.time, p.value]));
        return x.map((time) => {
          const value = byTime.get(time);
          return value === undefined ? null : row * 2 + value;
        });
      }),
    ];

    const height = Math.max(ROW_HEIGHT * series.length, ROW_HEIGHT);

    const stepped = uPlot.paths.stepped?.({ align: 1 });

    const options: uPlot.Options = {
      title,
      width: node.clientWidth || 600,
      height,
      cursor: { sync: { key: SYNC_KEY }, points: { show: false } },
      legend: { show: series.length > 1 },
      scales: { x: { time: true }, y: { range: [-0.5, series.length * 2] } },
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
  }, [title, series]);

  return <div className="chart chart--strip" ref={host} />;
}
