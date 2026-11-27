import type { ChannelLevels, LightingScheduleDto } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { CHANNEL_KEYS } from "@aquarium/view-model";
import type uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import {
  CHANNEL_COLORS,
  GRID,
  LINE_WIDTH,
  REFERENCE,
  TEXT_MUTED,
} from "@/components/atoms/Chart/chart-tokens";
import { useUplot, type ChartSpec } from "@/components/atoms/Chart/use-uplot";
import { MetricCard } from "@/components/molecules";
import "./LightingControl.scss";
import type { ReactNode } from "react";

const HEIGHT = 180;
const SPLITS = [0, 360, 720, 1080, 1440];

const hourLabel = (minute: number): string => {
  return `${String(Math.floor(minute / 60)).padStart(2, "0")}:00`;
};

const nowMarker = (minute: number) => {
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
};

export const SchedulePreview = ({
  schedule,
  nowMinute,
}: {
  schedule: LightingScheduleDto;
  nowMinute: number;
}): ReactNode => {
  const t = useT();

  const host = useUplot(
    (width): ChartSpec => {
      const minutes = schedule.white_pct.map(
        (_, index) => index * schedule.step_minutes,
      );
      const labels: Record<keyof ChannelLevels, string> = {
        red_pct: t("lighting.red"),
        green_pct: t("lighting.green"),
        blue_pct: t("lighting.blue"),
        white_pct: t("lighting.white"),
      };

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
            ...CHANNEL_KEYS.map((key) => ({
              label: labels[key],
              stroke: CHANNEL_COLORS[key],
              width: LINE_WIDTH,
              points: { show: false },
            })),
          ],
          hooks: { draw: [nowMarker(nowMinute)] },
        },
        data: [
          minutes,
          ...CHANNEL_KEYS.map((key) => schedule[key]),
        ] as uPlot.AlignedData,
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
};
