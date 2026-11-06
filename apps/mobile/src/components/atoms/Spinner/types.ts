import type { StyleProp, ViewStyle } from "react-native";

export type SpinnerVariant = "default" | "dots" | "hash" | "bar" | "beat";

export interface SpinnerProps {
  variant?: SpinnerVariant;
  size?: number;
  color?: string;
  style?: StyleProp<ViewStyle>;
}
