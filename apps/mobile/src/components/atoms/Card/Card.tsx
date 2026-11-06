import { cardStyle } from "@aquarium/theme";
import { View } from "react-native";
import type { CardProps } from "./types";

export type { CardPadding, CardProps, CardVariant, CardWidth } from "./types";

export function Card({
  children,
  variant = "elevated",
  padding = "md",
  width = "auto",
  style,
  ...rest
}: CardProps) {
  return (
    <View style={[cardStyle({ variant, padding, width }), style]} {...rest}>
      {children}
    </View>
  );
}
