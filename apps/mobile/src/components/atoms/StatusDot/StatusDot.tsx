import { cardStyles, statusDotStyle, statusLabelStyle } from "@aquarium/theme";
import { Text, View } from "react-native";
import type { StatusDotProps } from "./types";
import type { ReactNode } from "react";

export type { StatusDotProps, StatusTone } from "./types";

export const StatusDot = ({ tone, label }: StatusDotProps): ReactNode => {
  return (
    <View style={cardStyles.status}>
      <View style={statusDotStyle(tone)} />
      {label ? <Text style={statusLabelStyle(tone)}>{label}</Text> : null}
    </View>
  );
};
