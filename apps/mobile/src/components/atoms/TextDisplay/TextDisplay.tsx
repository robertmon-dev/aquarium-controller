import {
  spacing,
  textDisplayHelperStyle,
  textDisplayLabelStyle,
  textDisplayValueStyle,
} from "@aquarium/theme";
import { Text, View } from "react-native";
import type { TextDisplayProps } from "./types";
import type { ReactNode } from "react";

export type { TextDisplayProps, TextDisplayState } from "./types";

export const TextDisplay = ({
  label,
  children,
  state = "default",
  helperText,
  fullWidth = false,
  maxHeight,
  style,
}: TextDisplayProps): ReactNode => {
  return (
    <View
      style={[
        { gap: spacing.xxs },
        fullWidth ? { width: "100%" } : null,
        maxHeight === undefined ? null : { maxHeight },
        style,
      ]}
    >
      {label ? <Text style={textDisplayLabelStyle}>{label}</Text> : null}
      {typeof children === "string" ? (
        <Text style={textDisplayValueStyle(state)}>{children}</Text>
      ) : (
        children
      )}
      {helperText ? (
        <Text style={textDisplayHelperStyle}>{helperText}</Text>
      ) : null}
    </View>
  );
};
