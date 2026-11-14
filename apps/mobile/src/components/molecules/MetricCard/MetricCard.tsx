import { cardStyles } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Card, StatusDot } from "@/components/atoms";
import { MetricList } from "@/components/molecules/MetricList/MetricList";
import type { MetricCardProps } from "./types";
import type { ReactNode } from "react";

export type { MetricCardProps } from "./types";

export const MetricCard = ({
  title,
  status,
  statusLabel,
  metrics,
  children,
}: MetricCardProps): ReactNode => {
  return (
    <Card variant="elevated" padding="md" width="full">
      <View style={cardStyles.header}>
        <Text style={cardStyles.title}>{title}</Text>
        <StatusDot tone={status} label={statusLabel} />
      </View>
      {metrics ? <MetricList metrics={metrics} /> : null}
      {children}
    </Card>
  );
};
