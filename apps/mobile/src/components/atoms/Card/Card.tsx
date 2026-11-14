import { cardStyle } from "@aquarium/theme";
import { View } from "react-native";
import type { CardProps } from "./types";
import type { ReactNode } from "react";

export type { CardPadding, CardProps, CardVariant, CardWidth } from "./types";

export const Card = ({
  children,
  variant = "elevated",
  padding = "md",
  width = "auto",
  style,
  ...rest
}: CardProps): ReactNode => {
  return (
    <View style={[cardStyle({ variant, padding, width }), style]} {...rest}>
      {children}
    </View>
  );
};
