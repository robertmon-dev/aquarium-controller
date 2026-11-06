import type { TextStyle, ViewStyle } from "react-native";
import {
  colors,
  radius,
  spacing,
  typography,
} from "@/components/utility/theme";
import type { BadgeSize, BadgeVariant } from "./types";

const VARIANTS: Record<
  BadgeVariant,
  { container: ViewStyle; label: TextStyle }
> = {
  primary: {
    container: { backgroundColor: "rgba(122, 162, 247, 0.15)" },
    label: { color: colors.primary },
  },
  secondary: {
    container: { backgroundColor: "rgba(86, 95, 137, 0.25)" },
    label: { color: colors.textMain },
  },
  warning: {
    container: { backgroundColor: "rgba(255, 158, 100, 0.15)" },
    label: { color: colors.warning },
  },
  success: {
    container: { backgroundColor: "rgba(158, 206, 106, 0.15)" },
    label: { color: colors.success },
  },
  danger: {
    container: { backgroundColor: "rgba(247, 118, 142, 0.15)" },
    label: { color: colors.danger },
  },
  info: {
    container: { backgroundColor: "rgba(224, 175, 104, 0.15)" },
    label: { color: colors.info },
  },
  outline: {
    container: {
      backgroundColor: "transparent",
      borderWidth: 1,
      borderColor: colors.border,
    },
    label: { color: colors.textMuted },
  },
};

const SIZES: Record<BadgeSize, { height: number; font: number }> = {
  sm: { height: 18, font: typography.size.sm - 2 },
  md: { height: 22, font: typography.size.sm },
};

export function badgeContainerStyle(
  variant: BadgeVariant,
  size: BadgeSize,
): ViewStyle {
  return {
    alignSelf: "flex-start",
    alignItems: "center",
    justifyContent: "center",
    height: SIZES[size].height,
    paddingHorizontal: spacing.xs,
    borderRadius: radius.sm,
    ...VARIANTS[variant].container,
  };
}

export function badgeLabelStyle(
  variant: BadgeVariant,
  size: BadgeSize,
): TextStyle {
  return {
    fontSize: SIZES[size].font,
    fontWeight: typography.weight.medium,
    ...VARIANTS[variant].label,
  };
}
