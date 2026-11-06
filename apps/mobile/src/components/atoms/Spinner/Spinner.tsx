import { ActivityIndicator } from "react-native";
import { colors } from "@/components/utility/theme";
import type { SpinnerProps } from "./types";

export type { SpinnerProps, SpinnerVariant } from "./types";

export function Spinner({
  size = 24,
  color = colors.primary,
  style,
}: SpinnerProps) {
  return <ActivityIndicator size={size} color={color} style={style} />;
}
