import { badgeContainerStyle, badgeLabelStyle } from "@aquarium/theme";
import { Text, View } from "react-native";
import type { BadgeProps } from "./types";
import type { ReactNode } from "react";

export type { BadgeProps, BadgeSize, BadgeVariant } from "./types";

export const Badge = ({
  variant = "primary",
  size = "md",
  children,
  style,
}: BadgeProps): ReactNode => {
  return (
    <View style={[badgeContainerStyle(variant, size), style]}>
      {typeof children === "string" ? (
        <Text style={badgeLabelStyle(variant, size)}>{children}</Text>
      ) : (
        children
      )}
    </View>
  );
};
