import type { ReactNode } from "react";
import type { StyleProp, ViewProps, ViewStyle } from "react-native";
import type { CardPadding, CardVariant, CardWidth } from "@aquarium/theme";

export type { CardPadding, CardVariant, CardWidth };

export interface CardProps extends Omit<ViewProps, "style"> {
  children: ReactNode;
  variant?: CardVariant;
  padding?: CardPadding;
  width?: CardWidth;
  style?: StyleProp<ViewStyle>;
}
