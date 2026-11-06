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
import { HeroCard } from "./HeroCard";

export function TelemetryDashboard({
  snapshot,
}: {
  snapshot: TankTelemetryDto;
}) {
  const levels = waterLevelCard(snapshot.water_level);
  const cards = [
    filtrationCard(snapshot),
    co2Card(snapshot),
    atoCard(snapshot),
    lightingCard(snapshot),
    systemCard(snapshot),
  ];

  return (
    <View style={{ gap: spacing.sm }}>
      {snapshot.leak_detected ? (
        <Alert variant="danger" title="leak detected">
          water outside the tank — check the sump and the floor
        </Alert>
      ) : null}

      <HeroCard card={temperatureCard(snapshot)} />

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
