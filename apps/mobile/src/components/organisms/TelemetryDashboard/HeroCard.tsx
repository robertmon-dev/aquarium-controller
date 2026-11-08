import { cardStyles } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Card, StatusDot } from "@/components/atoms";
import { MetricList } from "@/components/molecules";
import type { HeroCardData } from "@aquarium/view-model";

export function HeroCard({ card }: { card: HeroCardData }) {
  return (
    <Card variant="levitating" padding="md" width="full">
      <View style={cardStyles.header}>
        <Text style={cardStyles.title}>{card.title}</Text>
        <StatusDot tone={card.status} label={card.statusLabel} />
      </View>

      <View style={cardStyles.heroRow}>
        <Text style={cardStyles.heroValue}>{card.heroValue}</Text>
        <Text style={cardStyles.heroUnit}>{card.heroUnit}</Text>
      </View>

      <MetricList metrics={card.metrics} />
    </Card>
  );
}
