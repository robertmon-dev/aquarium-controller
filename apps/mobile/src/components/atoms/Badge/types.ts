import type { ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";
import type { BadgeSize, BadgeVariant } from "@aquarium/theme";

export type { BadgeSize, BadgeVariant };

export interface BadgeProps {
  variant?: BadgeVariant;
  size?: BadgeSize;
  children: ReactNode;
  style?: StyleProp<ViewStyle>;
}
