import { colors, radius, spacing, typography } from "@aquarium/theme";
import { Text, View } from "react-native";
import type { MeterProps } from "./types";

export type { MeterProps } from "./types";

const TRACK_HEIGHT = 6;

export function Meter({ value, label, valueText }: MeterProps) {
  const clamped = Math.min(1, Math.max(0, value));

  return (
    <View style={{ gap: spacing.xxs }}>
      <View
        style={{
          flexDirection: "row",
          justifyContent: "space-between",
          alignItems: "baseline",
        }}
      >
        <Text
          style={{
            color: colors.textMuted,
            fontFamily: typography.fontFamily,
            fontSize: typography.size.sm,
          }}
        >
          {label}
        </Text>
        <Text
          style={{
            color: colors.textMain,
            fontFamily: typography.fontFamily,
            fontSize: typography.size.sm,
          }}
        >
          {valueText}
        </Text>
      </View>

      <View
        accessibilityRole="progressbar"
        accessibilityValue={{ now: Math.round(clamped * 100), min: 0, max: 100 }}
        style={{
          height: TRACK_HEIGHT,
          borderRadius: radius.sm,
          backgroundColor: colors.bgField,
          overflow: "hidden",
        }}
      >
        <View
          style={{
            width: `${clamped * 100}%`,
            height: "100%",
            borderRadius: radius.sm,
            backgroundColor: colors.warning,
          }}
        />
      </View>
    </View>
  );
}
