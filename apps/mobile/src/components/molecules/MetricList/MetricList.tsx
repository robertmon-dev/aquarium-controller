import { screenStyles, spacing, textDisplayValueStyle } from "@aquarium/theme";
import { Text, View } from "react-native";
import type { MetricListProps } from "./types";
import type { ReactNode } from "react";

export type { Metric, MetricListProps } from "./types";

export const MetricList = ({ metrics, style }: MetricListProps): ReactNode => {
  return (
    <View style={[{ gap: spacing.xs / 2 }, style]}>
      {metrics.map((metric) => (
        <View key={metric.label} style={screenStyles.metricRow}>
          <Text style={screenStyles.mutedText}>{metric.label}</Text>
          <Text style={textDisplayValueStyle(metric.state ?? "default")}>
            {metric.value}
          </Text>
        </View>
      ))}
    </View>
  );
};
