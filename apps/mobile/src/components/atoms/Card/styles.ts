import type { ViewStyle } from "react-native";
import { colors, radius, shadows, spacing } from "@/components/utility/theme";
import type { CardPadding, CardVariant, CardWidth } from "./types";

const PADDING: Record<CardPadding, number> = {
  none: spacing.none,
  sm: spacing.sm,
  md: spacing.md,
  lg: spacing.lg,
};

const WIDTH: Record<CardWidth, ViewStyle> = {
  auto: {},
  wide: { maxWidth: 720, width: "100%" },
  full: { width: "100%" },
};

const VARIANTS: Record<CardVariant, ViewStyle> = {
  elevated: { backgroundColor: colors.bgSurface, boxShadow: shadows.surfaceSm },
  outlined: {
    backgroundColor: colors.bgSurface,
    borderWidth: 1,
    borderColor: colors.border,
  },
  flat: { backgroundColor: colors.bgSurface },
  transparent: { backgroundColor: "transparent" },
  levitating: {
    backgroundColor: colors.bgSurface,
    boxShadow: shadows.surfaceMd,
  },
  "levitating-transparent": {
    backgroundColor: "transparent",
    boxShadow: shadows.surfaceMd,
  },
  floating: { backgroundColor: colors.bgSurface, boxShadow: shadows.surfaceMd },
  contact: {
    backgroundColor: colors.bgSurface,
    borderWidth: 1,
    borderColor: colors.border,
    boxShadow: shadows.surfaceSm,
  },
};

export function cardStyle({
  variant,
  padding,
  width,
}: {
  variant: CardVariant;
  padding: CardPadding;
  width: CardWidth;
}): ViewStyle {
  return {
    borderRadius: radius.md,
    padding: PADDING[padding],
    ...WIDTH[width],
    ...VARIANTS[variant],
  };
}
