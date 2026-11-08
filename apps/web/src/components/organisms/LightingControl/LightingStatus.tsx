import type { LightingMetricsDto } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { formatDuration } from "@aquarium/view-model";
import { Meter } from "@/components/atoms";
import { MetricCard } from "@/components/molecules";
import "./LightingControl.scss";

export function LightingStatus({
  lighting,
  remaining,
  total,
}: {
  lighting: LightingMetricsDto;
  remaining: number | null;
  total: number | null;
}) {
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
          {t(`profiles.${lighting.profile}`)}
        </p>

        <div className="lighting-status__channels">
          {channels.map((channel) => (
            <div className="lighting-status__channel" key={channel.name}>
              <span className="lighting-status__reading">
                <span className="lighting-status__value">{channel.pct}</span>
                <span className="lighting-status__unit">%</span>
              </span>
              <span className="lighting-status__name">{channel.name}</span>
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
}
