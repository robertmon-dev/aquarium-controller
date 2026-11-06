import type { TextStyle } from "react-native";
import { colors, typography } from "@/components/utility/theme";
import type { TextDisplayState } from "./types";

const STATE_COLOR: Record<TextDisplayState, string> = {
  default: colors.textMain,
  error: colors.danger,
  success: colors.success,
  warning: colors.warning,
  info: colors.info,
};

export function textDisplayValueStyle(state: TextDisplayState): TextStyle {
  return { color: STATE_COLOR[state], fontSize: typography.size.md };
}

export const textDisplayLabelStyle: TextStyle = {
  color: colors.textMuted,
  fontSize: typography.size.sm,
};

export const textDisplayHelperStyle: TextStyle = {
  color: colors.textMuted,
  fontSize: typography.size.sm,
};
