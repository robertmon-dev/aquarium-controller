import type { LightingScheduleDto } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import type uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import {
  GRID,
  LINE_WIDTH,
  REFERENCE,
  SERIES,
  TEXT_MUTED,
} from "@/components/atoms/Chart/chart-tokens";
import { useUplot, type ChartSpec } from "@/components/atoms/Chart/use-uplot";
import { MetricCard } from "@/components/molecules";
import "./LightingControl.scss";

const HEIGHT = 180;
const SPLITS = [0, 360, 720, 1080, 1440];

function hourLabel(minute: number): string {
  return `${String(Math.floor(minute / 60)).padStart(2, "0")}:00`;
}

function nowMarker(minute: number) {
  return (self: uPlot) => {
    const x = self.valToPos(minute, "x", true);
    const { ctx } = self;

    ctx.save();
    ctx.strokeStyle = REFERENCE;
    ctx.setLineDash([4, 4]);
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(x, self.bbox.top);
    ctx.lineTo(x, self.bbox.top + self.bbox.height);
    ctx.stroke();
    ctx.restore();
  };
}

export function SchedulePreview({
  schedule,
  nowMinute,
}: {
  schedule: LightingScheduleDto;
  nowMinute: number;
}) {
  const t = useT();

  const host = useUplot(
    (width): ChartSpec => {
      const minutes = schedule.white_pct.map(
        (_, index) => index * schedule.step_minutes,
      );

      return {
        options: {
          width,
          height: HEIGHT,
          cursor: { points: { show: false } },
          legend: { show: true },
          scales: {
            x: { time: false, range: [0, 1440] },
            y: { range: [0, 100] },
          },
          axes: [
            {
              stroke: TEXT_MUTED,
              grid: { stroke: GRID, width: 1 },
              splits: () => SPLITS,
              values: (_, splits) => splits.map(hourLabel),
            },
            {
              stroke: TEXT_MUTED,
              grid: { stroke: GRID, width: 1 },
              label: "%",
            },
          ],
          series: [
            {},
            {
              label: t("lighting.white"),
              stroke: SERIES[0],
              width: LINE_WIDTH,
              points: { show: false },
            },
            {
              label: t("lighting.rgb"),
              stroke: SERIES[1],
              width: LINE_WIDTH,
              points: { show: false },
            },
          ],
          hooks: { draw: [nowMarker(nowMinute)] },
        },
        data: [
          minutes,
          schedule.white_pct,
          schedule.rgb_pct,
        ] as unknown as uPlot.AlignedData,
      };
    },
    [schedule, nowMinute, t],
  );

  return (
    <MetricCard
      title={t("lighting.schedulePreview")}
      status="ok"
      statusLabel={hourLabel(nowMinute - (nowMinute % 60))}
    >
      <div className="chart" ref={host} />
    </MetricCard>
  );
}
