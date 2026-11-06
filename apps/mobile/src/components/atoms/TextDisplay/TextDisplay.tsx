import {
  spacing,
  textDisplayHelperStyle,
  textDisplayLabelStyle,
  textDisplayValueStyle,
} from "@aquarium/theme";
import { Text, View } from "react-native";
import type { TextDisplayProps } from "./types";

export type { TextDisplayProps, TextDisplayState } from "./types";

export function TextDisplay({
  label,
  children,
  state = "default",
  helperText,
  fullWidth = false,
  maxHeight,
  style,
}: TextDisplayProps) {
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
}
