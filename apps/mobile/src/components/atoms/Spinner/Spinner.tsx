import { colors } from "@aquarium/theme";
import { ActivityIndicator } from "react-native";
import type { SpinnerProps } from "./types";
import type { ReactNode } from "react";

export type { SpinnerProps, SpinnerVariant } from "./types";

export const Spinner = ({
  size = 24,
  color = colors.primary,
  style,
}: SpinnerProps): ReactNode => {
  return <ActivityIndicator size={size} color={color} style={style} />;
};
