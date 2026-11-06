import { Text, View } from "react-native";
import { badgeContainerStyle, badgeLabelStyle } from "./styles";
import type { BadgeProps } from "./types";

export type { BadgeProps, BadgeSize, BadgeVariant } from "./types";

export function Badge({
  variant = "primary",
  size = "md",
  children,
  style,
}: BadgeProps) {
  return (
    <View style={[badgeContainerStyle(variant, size), style]}>
      {typeof children === "string" ? (
        <Text style={badgeLabelStyle(variant, size)}>{children}</Text>
      ) : (
        children
      )}
    </View>
  );
}
