import type { TextStyle, ViewStyle } from "react-native";
import {
  colors,
  radius,
  spacing,
  typography,
} from "@/components/utility/theme";
import type { ButtonSize, ButtonVariant } from "./types";

const HEIGHT: Record<ButtonSize, number> = { sm: 32, md: 40, lg: 48 };
const FONT: Record<ButtonSize, number> = {
  sm: typography.size.sm,
  md: typography.size.md,
  lg: typography.size.lg,
};

type VariantStyle = { container: ViewStyle; label: TextStyle };

const VARIANTS: Record<ButtonVariant, VariantStyle> = {
  primary: {
    container: {
      backgroundColor: colors.primary,
      borderColor: "rgba(255, 255, 255, 0.1)",
      borderWidth: 1,
    },
    label: { color: "#ffffff" },
  },
  secondary: {
    container: {
      backgroundColor: colors.bgSurface,
      borderColor: "rgba(86, 95, 137, 0.3)",
      borderWidth: 2,
    },
    label: { color: colors.primary },
  },
  outline: {
    container: {
      backgroundColor: "transparent",
      borderColor: colors.primary,
      borderWidth: 2.5,
    },
    label: { color: colors.primary },
  },
  ghost: {
    container: {
      backgroundColor: "transparent",
      borderColor: "transparent",
      borderWidth: 0,
    },
    label: { color: colors.textMuted },
  },
  danger: {
    container: {
      backgroundColor: "rgba(247, 118, 142, 0.1)",
      borderColor: "rgba(247, 118, 142, 0.5)",
      borderWidth: 2.5,
    },
    label: { color: colors.danger },
  },
  success: {
    container: {
      backgroundColor: colors.success,
      borderColor: "rgba(255, 255, 255, 0.2)",
      borderWidth: 1,
    },
    label: { color: colors.textOnAccent },
  },
  purple: {
    container: {
      backgroundColor: "#bb9af7",
      borderColor: "rgba(255, 255, 255, 0.1)",
      borderWidth: 1,
    },
    label: { color: "#ffffff" },
  },
  select: {
    container: {
      backgroundColor: colors.bgInput,
      borderColor: colors.border,
      borderWidth: 1,
      borderRadius: radius.sm,
    },
    label: { color: colors.textMain },
  },
};

const GRADIENTS: Partial<Record<ButtonVariant, readonly [string, string]>> = {
  primary: ["#92b3f8", "#92b3f8"],
  success: ["#9ece6a", "#92c857"],
  purple: ["#bb9af7", "#9b6bf3"],
};

export function buttonGradient(
  variant: ButtonVariant,
): readonly [string, string] | null {
  return GRADIENTS[variant] ?? null;
}

export function buttonContainerStyle({
  variant,
  size,
  fullWidth,
  isIcon,
  pressed,
  disabled,
}: {
  variant: ButtonVariant;
  size: ButtonSize;
  fullWidth: boolean;
  isIcon: boolean;
  pressed: boolean;
  disabled: boolean;
}): ViewStyle {
  const height = HEIGHT[size];

  return {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "center",
    gap: spacing.xs,
    height,
    paddingHorizontal: isIcon ? 0 : spacing.sm,
    width: isIcon ? height : fullWidth ? "100%" : undefined,
    borderRadius: isIcon ? height / 2 : radius.mdL,
    opacity: disabled ? 0.5 : pressed ? 0.85 : 1,
    ...VARIANTS[variant].container,
  };
}

export function buttonLabelStyle(
  variant: ButtonVariant,
  size: ButtonSize,
): TextStyle {
  return {
    fontSize: FONT[size],
    fontWeight: typography.weight.medium,
    ...VARIANTS[variant].label,
  };
}
