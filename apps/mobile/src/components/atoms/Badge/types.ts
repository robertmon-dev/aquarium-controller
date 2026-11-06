import type { ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";

export type BadgeVariant =
  | "primary"
  | "secondary"
  | "warning"
  | "success"
  | "danger"
  | "info"
  | "outline";

export type BadgeSize = "sm" | "md";

export interface BadgeProps {
  variant?: BadgeVariant;
  size?: BadgeSize;
  children: ReactNode;
  style?: StyleProp<ViewStyle>;
}
