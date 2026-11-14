import { chipContainerStyle, chipLabelStyle } from "@aquarium/theme";
import { Pressable, Text, View } from "react-native";
import type { ChipProps } from "./types";
import type { ReactNode } from "react";

export type { ChipProps, ChipVariant } from "./types";

export const Chip = ({
  label,
  variant = "default",
  selected = false,
  onPress,
  style,
}: ChipProps): ReactNode => {
  const container = chipContainerStyle(variant, selected);
  const text = <Text style={chipLabelStyle(variant, selected)}>{label}</Text>;

  if (!onPress) {
    return <View style={[container, style]}>{text}</View>;
  }

  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ selected }}
      onPress={onPress}
      style={({ pressed }) => [
        container,
        pressed ? { opacity: 0.85 } : null,
        style,
      ]}
    >
      {text}
    </Pressable>
  );
};
