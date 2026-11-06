import type { ViewStyle } from "react-native";
import { colors, radius, spacing } from "@/components/utility/theme";
import type { AlertVariant } from "./types";

const ACCENT: Record<AlertVariant, string> = {
  info: colors.info,
  success: colors.success,
  warning: colors.warning,
  danger: colors.danger,
};

const TINT: Record<AlertVariant, string> = {
  info: "rgba(224, 175, 104, 0.12)",
  success: "rgba(158, 206, 106, 0.12)",
  warning: "rgba(255, 158, 100, 0.12)",
  danger: "rgba(247, 118, 142, 0.12)",
};

export function alertAccent(variant: AlertVariant): string {
  return ACCENT[variant];
}

export function alertStyle(variant: AlertVariant): ViewStyle {
  return {
    flexDirection: "row",
    alignItems: "flex-start",
    gap: spacing.xs,
    padding: spacing.sm,
    borderRadius: radius.sm,
    borderLeftWidth: 3,
    borderLeftColor: ACCENT[variant],
    backgroundColor: TINT[variant],
  };
}
