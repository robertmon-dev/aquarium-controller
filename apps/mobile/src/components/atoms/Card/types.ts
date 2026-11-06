import type { ReactNode } from "react";
import type { StyleProp, ViewProps, ViewStyle } from "react-native";

export type CardVariant =
  | "elevated"
  | "outlined"
  | "flat"
  | "transparent"
  | "levitating"
  | "levitating-transparent"
  | "floating"
  | "contact";

export type CardPadding = "none" | "sm" | "md" | "lg";
export type CardWidth = "auto" | "wide" | "full";

export interface CardProps extends Omit<ViewProps, "style"> {
  children: ReactNode;
  variant?: CardVariant;
  padding?: CardPadding;
  width?: CardWidth;
  style?: StyleProp<ViewStyle>;
}
