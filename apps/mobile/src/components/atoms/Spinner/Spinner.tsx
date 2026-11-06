import { colors } from "@aquarium/theme";
import { ActivityIndicator } from "react-native";
import type { SpinnerProps } from "./types";

export type { SpinnerProps, SpinnerVariant } from "./types";

export function Spinner({
  size = 24,
  color = colors.primary,
  style,
}: SpinnerProps) {
  return <ActivityIndicator size={size} color={color} style={style} />;
}
