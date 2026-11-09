import type { LightingMetricsDto } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { formatDuration } from "@aquarium/view-model";
import { Meter } from "@/components/atoms";
import { MetricCard } from "@/components/molecules";
import "./LightingControl.scss";

export const LightingStatus = ({
  lighting,
  remaining,
  total,
  manualChannels,
}: {
  lighting: LightingMetricsDto;
  remaining: number | null;
  total: number | null;
  manualChannels: boolean;
}) => {
  const t = useT();
  const active = lighting.override_active;

  const channels: { name: string; pct: number }[] = [
    { name: t("lighting.white"), pct: lighting.channel_white_pct },
    { name: t("lighting.rgb"), pct: lighting.channel_rgb_pct },
  ];

  return (
    <MetricCard
      title={t("lighting.currentState")}
      status={active ? "degraded" : "ok"}
      statusLabel={active ? t("lighting.override") : t("lighting.onSchedule")}
    >
      <div className="lighting-status">
        <p className="lighting-status__profile">
          {manualChannels
            ? t("lighting.custom")
            : t(`profiles.${lighting.profile}`)}
        </p>

        <div className="lighting-status__channels">
          {channels.map((channel) => (
            <div className="lighting-status__channel" key={channel.name}>
              <span className="lighting-status__name">{channel.name}</span>

              <span className="lighting-status__reading">
                <span className="lighting-status__value">{channel.pct}</span>
                <span className="lighting-status__unit">%</span>
              </span>

              <div
                className="lighting-status__bar"
                role="meter"
                aria-label={channel.name}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={channel.pct}
              >
                <div
                  className="lighting-status__bar-fill"
                  style={{ width: `${channel.pct}%` }}
                />
              </div>
            </div>
          ))}
        </div>

        {remaining !== null && total !== null && total > 0 ? (
          <Meter
            value={remaining / total}
            label={t("lighting.expiresIn")}
            valueText={formatDuration(remaining)}
          />
        ) : null}
      </div>
    </MetricCard>
  );
};
