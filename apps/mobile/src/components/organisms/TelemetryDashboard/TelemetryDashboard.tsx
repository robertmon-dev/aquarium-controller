import type { TankTelemetryDto } from "@aquarium/api-client";
import { spacing } from "@aquarium/theme";
import { View } from "react-native";
import { Alert } from "@/components/atoms";
import { MetricCard, WaterLevelGrid } from "@/components/molecules";
import {
  atoCard,
  co2Card,
  filtrationCard,
  lightingCard,
  systemCard,
  temperatureCard,
} from "@/components/utility/cards";
import { waterLevelCard } from "@/components/utility/water-level";
import { useT } from "@/i18n/use-locale";
import { HeroCard } from "./HeroCard";

export function TelemetryDashboard({
  snapshot,
}: {
  snapshot: TankTelemetryDto;
}) {
  const t = useT();
  const levels = waterLevelCard(snapshot.water_level, t);
  const cards = [
    filtrationCard(snapshot, t),
    co2Card(snapshot, t),
    atoCard(snapshot, t),
    lightingCard(snapshot, t),
    systemCard(snapshot, t),
  ];

  return (
    <View style={{ gap: spacing.sm }}>
      {snapshot.leak_detected ? (
        <Alert variant="danger" title={t("telemetry.leakTitle")}>
          {t("telemetry.leakBody")}
        </Alert>
      ) : null}

      <HeroCard card={temperatureCard(snapshot, t)} />

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
    </View>
  );
}
