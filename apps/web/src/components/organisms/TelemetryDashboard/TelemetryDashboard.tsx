import type { TankTelemetryDto } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import {
  atoCard,
  co2Card,
  filtrationCard,
  floodMessage,
  lightingCard,
  pumpsCard,
  systemCard,
  tdsCard,
  temperatureCard,
  waterLevelCard,
} from "@aquarium/view-model";
import { Alert } from "@/components/atoms";
import { MetricCard, MetricGrid, WaterLevelGrid } from "@/components/molecules";
import { HeroCard } from "./HeroCard";

export function TelemetryDashboard({
  snapshot,
}: {
  snapshot: TankTelemetryDto;
}) {
  const t = useT();
  const levels = waterLevelCard(snapshot.water_level, t);
  const alarm = floodMessage(snapshot, t);
  const cards = [
    filtrationCard(snapshot, t),
    co2Card(snapshot, t),
    tdsCard(snapshot, t),
    atoCard(snapshot, t),
    pumpsCard(snapshot, t),
    lightingCard(snapshot, t),
    systemCard(snapshot, t),
  ];

  return (
    <>
      {alarm ? (
        <Alert variant="danger" title={t("telemetry.leakTitle")}>
          {alarm}
        </Alert>
      ) : null}

      {snapshot.system_mode === "normal" ? null : (
        <Alert variant="warning" title={t("cards.system.title")}>
          {snapshot.system_mode}
        </Alert>
      )}

      <HeroCard card={temperatureCard(snapshot, t)} />

      <MetricGrid>
        <MetricCard
          title={levels.title}
          status={levels.status}
          statusLabel={levels.statusLabel}
        >
          <WaterLevelGrid chips={levels.chips} />
        </MetricCard>

        {cards.map((card) => (
          <MetricCard
            key={card.title}
            title={card.title}
            status={card.status}
            statusLabel={card.statusLabel}
            metrics={card.metrics}
          />
        ))}
      </MetricGrid>
    </>
  );
}
