import { cardStyles, statusDotStyle, statusLabelStyle } from "@aquarium/theme";
import { Text, View } from "react-native";
import type { StatusDotProps } from "./types";

export type { StatusDotProps, StatusTone } from "./types";

export function StatusDot({ tone, label }: StatusDotProps) {
  return (
    <View style={cardStyles.status}>
      <View style={statusDotStyle(tone)} />
      {label ? <Text style={statusLabelStyle(tone)}>{label}</Text> : null}
    </View>
  );
}
