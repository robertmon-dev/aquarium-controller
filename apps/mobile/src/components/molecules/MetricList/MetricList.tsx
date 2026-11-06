import { Text, View } from "react-native";
import { colors, spacing, typography } from "@/components/utility/theme";
import { textDisplayValueStyle } from "@/components/atoms/TextDisplay/styles";
import type { MetricListProps } from "./types";

export type { Metric, MetricListProps } from "./types";

export function MetricList({ metrics, style }: MetricListProps) {
  return (
    <View style={[{ gap: spacing.xs / 2 }, style]}>
      {metrics.map((metric) => (
        <View
          key={metric.label}
          style={{
            flexDirection: "row",
            justifyContent: "space-between",
            gap: spacing.sm,
          }}
        >
          <Text
            style={{ color: colors.textMuted, fontSize: typography.size.md }}
          >
            {metric.label}
          </Text>
          <Text style={textDisplayValueStyle(metric.state ?? "default")}>
            {metric.value}
          </Text>
        </View>
      ))}
    </View>
  );
}
