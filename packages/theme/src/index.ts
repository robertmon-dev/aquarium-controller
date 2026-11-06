import type { TextStyle, ViewStyle } from "react-native";

export const palette = {
  red: "#f7768e",
  orange: "#ff9e64",
  yellow: "#e0af68",
  beige: "#cfc9c2",

  green: "#9ece6a",
  teal: "#73daca",
  cyanPale: "#b4f9f8",
  cyan: "#2ac3de",
  cyanDim: "#41a6b5",

  blueSky: "#7dcfff",
  blue: "#7aa2f7",
  purple: "#bb9af7",
  lavender: "#c0caf5",

  greyBlue: "#a9b1d6",
  greyMuted: "#9aa5ce",
  slate: "#565f89",
  storm: "#414868",

  bgNight: "#1a1b26",
  bgStorm: "#24283b",
} as const;

export const colors = {
  bgApp: palette.bgNight,
  bgSurface: palette.bgStorm,
  bgSidebar: palette.storm,
  bgInput: "#161720",

  primary: palette.blue,
  primaryHover: "#92b3f8",
  highlight: palette.cyan,
  highlightDim: palette.cyanDim,
  danger: palette.red,
  success: palette.green,
  warning: palette.orange,
  info: palette.yellow,

  textMain: palette.lavender,
  textMuted: palette.slate,
  textOnAccent: "#15161e",
  textLink: palette.cyanPale,

  border: "rgba(86, 95, 137, 0.4)",
} as const;

export const alpha = {
  primary15: "rgba(122, 162, 247, 0.15)",
  slate25: "rgba(86, 95, 137, 0.25)",
  slate30: "rgba(86, 95, 137, 0.3)",
  danger10: "rgba(247, 118, 142, 0.1)",
  danger12: "rgba(247, 118, 142, 0.12)",
  danger15: "rgba(247, 118, 142, 0.15)",
  danger50: "rgba(247, 118, 142, 0.5)",
  success12: "rgba(158, 206, 106, 0.12)",
  success15: "rgba(158, 206, 106, 0.15)",
  warning12: "rgba(255, 158, 100, 0.12)",
  warning15: "rgba(255, 158, 100, 0.15)",
  info12: "rgba(224, 175, 104, 0.12)",
  info15: "rgba(224, 175, 104, 0.15)",
  white10: "rgba(255, 255, 255, 0.1)",
  white20: "rgba(255, 255, 255, 0.2)",
} as const;

export const spacing = {
  none: 0,
  xxs: 2,
  xs: 8,
  sm: 12,
  md: 24,
  mdL: 28,
  lg: 40,
  xl: 64,
} as const;

export const radius = { sm: 6, md: 12, mdL: 16, lg: 40 } as const;

export const typography = {
  fontFamily: "JetBrains Mono",
  size: { xs: 10, sm: 12, md: 14, lg: 18, xl: 24 },
  weight: { regular: "400", medium: "500", bold: "700" },
} as const;

export const shadows = {
  primarySm: [
    {
      offsetX: 0,
      offsetY: 2,
      blurRadius: 4,
      spreadDistance: -1,
      color: "rgba(122, 162, 247, 0.4)",
    },
    {
      offsetX: 0,
      offsetY: 4,
      blurRadius: 8,
      spreadDistance: -2,
      color: "rgba(122, 162, 247, 0.2)",
    },
  ],
  surfaceSm: [
    {
      offsetX: 0,
      offsetY: 1,
      blurRadius: 2,
      spreadDistance: 0,
      color: "rgba(0, 0, 0, 0.4)",
    },
    {
      offsetX: 0,
      offsetY: 2,
      blurRadius: 4,
      spreadDistance: -1,
      color: "rgba(0, 0, 0, 0.3)",
    },
    {
      offsetX: 0,
      offsetY: 1,
      blurRadius: 0,
      spreadDistance: 0,
      color: alpha.primary15,
      inset: true,
    },
  ],
  surfaceMd: [
    {
      offsetX: 0,
      offsetY: 4,
      blurRadius: 6,
      spreadDistance: -1,
      color: "rgba(0, 0, 0, 0.5)",
    },
    {
      offsetX: 0,
      offsetY: 12,
      blurRadius: 24,
      spreadDistance: -6,
      color: "rgba(0, 0, 0, 0.4)",
    },
    {
      offsetX: 0,
      offsetY: 1,
      blurRadius: 0,
      spreadDistance: 0,
      color: alpha.primary15,
      inset: true,
    },
  ],
} as const;

export const theme = {
  palette,
  colors,
  alpha,
  spacing,
  radius,
  typography,
  shadows,
} as const;

export type ButtonVariant =
  | "primary"
  | "secondary"
  | "outline"
  | "ghost"
  | "danger"
  | "success"
  | "purple"
  | "select";

export type ButtonSize = "sm" | "md" | "lg";

const BUTTON_HEIGHT: Record<ButtonSize, number> = { sm: 32, md: 40, lg: 48 };

const BUTTON_FONT: Record<ButtonSize, number> = {
  sm: typography.size.sm,
  md: typography.size.md,
  lg: typography.size.lg,
};

const BUTTON_VARIANTS: Record<
  ButtonVariant,
  { container: ViewStyle; label: TextStyle }
> = {
  primary: {
    container: {
      backgroundColor: colors.primary,
      borderColor: alpha.white10,
      borderWidth: 1,
    },
    label: { color: "#ffffff" },
  },
  secondary: {
    container: {
      backgroundColor: colors.bgSurface,
      borderColor: alpha.slate30,
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
      backgroundColor: alpha.danger10,
      borderColor: alpha.danger50,
      borderWidth: 2.5,
    },
    label: { color: colors.danger },
  },
  success: {
    container: {
      backgroundColor: colors.success,
      borderColor: alpha.white20,
      borderWidth: 1,
    },
    label: { color: colors.textOnAccent },
  },
  purple: {
    container: {
      backgroundColor: palette.purple,
      borderColor: alpha.white10,
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

const BUTTON_GRADIENTS: Partial<
  Record<ButtonVariant, readonly [string, string]>
> = {
  primary: [colors.primaryHover, colors.primaryHover],
  success: ["#9ece6a", "#92c857"],
  purple: ["#bb9af7", "#9b6bf3"],
};

export function buttonGradient(
  variant: ButtonVariant,
): readonly [string, string] | null {
  return BUTTON_GRADIENTS[variant] ?? null;
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
  const height = BUTTON_HEIGHT[size];

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
    overflow: "hidden",
    ...BUTTON_VARIANTS[variant].container,
  };
}

export function buttonLabelStyle(
  variant: ButtonVariant,
  size: ButtonSize,
): TextStyle {
  return {
    fontSize: BUTTON_FONT[size],
    fontWeight: typography.weight.medium,
    ...BUTTON_VARIANTS[variant].label,
  };
}

export const inputStyles = {
  wrapper: { gap: spacing.xxs } satisfies ViewStyle,
  label: {
    color: colors.textMain,
    fontSize: typography.size.sm,
    fontWeight: typography.weight.medium,
  } satisfies TextStyle,
  field: {
    flexDirection: "row",
    alignItems: "center",
    gap: spacing.xs,
    minHeight: 40,
    paddingHorizontal: spacing.sm,
    backgroundColor: colors.bgInput,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: radius.sm,
  } satisfies ViewStyle,
  fieldFocused: { borderColor: colors.primary } satisfies ViewStyle,
  fieldError: { borderColor: colors.danger } satisfies ViewStyle,
  control: {
    flex: 1,
    color: colors.textMain,
    fontSize: typography.size.md,
    paddingVertical: spacing.xs,
  } satisfies TextStyle,
  helper: {
    color: colors.textMuted,
    fontSize: typography.size.sm,
  } satisfies TextStyle,
  error: {
    color: colors.danger,
    fontSize: typography.size.sm,
  } satisfies TextStyle,
} as const;

export type CardVariant =
  | "elevated"
  | "outlined"
  | "flat"
  | "transparent"
  | "levitating"
  | "levitating-transparent"
  | "floating"
  | "contact";

export type CardPadding = "none" | "sm" | "md" | "lg";
export type CardWidth = "auto" | "wide" | "full";

const CARD_PADDING: Record<CardPadding, number> = {
  none: spacing.none,
  sm: spacing.sm,
  md: spacing.md,
  lg: spacing.lg,
};

const CARD_WIDTH: Record<CardWidth, ViewStyle> = {
  auto: {},
  wide: { maxWidth: 720, width: "100%" },
  full: { width: "100%" },
};

const CARD_VARIANTS: Record<CardVariant, ViewStyle> = {
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
    padding: CARD_PADDING[padding],
    ...CARD_WIDTH[width],
    ...CARD_VARIANTS[variant],
  };
}

export type AlertVariant = "info" | "success" | "warning" | "danger";

const ALERT_ACCENT: Record<AlertVariant, string> = {
  info: colors.info,
  success: colors.success,
  warning: colors.warning,
  danger: colors.danger,
};

const ALERT_TINT: Record<AlertVariant, string> = {
  info: alpha.info12,
  success: alpha.success12,
  warning: alpha.warning12,
  danger: alpha.danger12,
};

export function alertAccent(variant: AlertVariant): string {
  return ALERT_ACCENT[variant];
}

export function alertStyle(variant: AlertVariant): ViewStyle {
  return {
    flexDirection: "row",
    alignItems: "flex-start",
    gap: spacing.xs,
    padding: spacing.sm,
    borderRadius: radius.sm,
    borderLeftWidth: 3,
    borderLeftColor: ALERT_ACCENT[variant],
    backgroundColor: ALERT_TINT[variant],
  };
}

export function alertTitleStyle(variant: AlertVariant): TextStyle {
  return {
    color: ALERT_ACCENT[variant],
    fontSize: typography.size.md,
    fontWeight: typography.weight.bold,
  };
}

export const alertBodyStyle: TextStyle = {
  color: colors.textMain,
  fontSize: typography.size.md,
};

export const alertCloseStyle: TextStyle = {
  color: colors.textMuted,
  fontSize: typography.size.lg,
};

export type BadgeVariant =
  | "primary"
  | "secondary"
  | "warning"
  | "success"
  | "danger"
  | "info"
  | "outline";

export type BadgeSize = "sm" | "md";

const BADGE_VARIANTS: Record<
  BadgeVariant,
  { container: ViewStyle; label: TextStyle }
> = {
  primary: {
    container: { backgroundColor: alpha.primary15 },
    label: { color: colors.primary },
  },
  secondary: {
    container: { backgroundColor: alpha.slate25 },
    label: { color: colors.textMain },
  },
  warning: {
    container: { backgroundColor: alpha.warning15 },
    label: { color: colors.warning },
  },
  success: {
    container: { backgroundColor: alpha.success15 },
    label: { color: colors.success },
  },
  danger: {
    container: { backgroundColor: alpha.danger15 },
    label: { color: colors.danger },
  },
  info: {
    container: { backgroundColor: alpha.info15 },
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

const BADGE_SIZES: Record<BadgeSize, { height: number; font: number }> = {
  sm: { height: 18, font: typography.size.xs },
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
    height: BADGE_SIZES[size].height,
    paddingHorizontal: spacing.xs,
    borderRadius: radius.sm,
    ...BADGE_VARIANTS[variant].container,
  };
}

export function badgeLabelStyle(
  variant: BadgeVariant,
  size: BadgeSize,
): TextStyle {
  return {
    fontSize: BADGE_SIZES[size].font,
    fontWeight: typography.weight.medium,
    ...BADGE_VARIANTS[variant].label,
  };
}

export type TextDisplayState =
  | "default"
  | "error"
  | "success"
  | "warning"
  | "info";

const TEXT_DISPLAY_COLOR: Record<TextDisplayState, string> = {
  default: colors.textMain,
  error: colors.danger,
  success: colors.success,
  warning: colors.warning,
  info: colors.info,
};

export function textDisplayValueStyle(state: TextDisplayState): TextStyle {
  return { color: TEXT_DISPLAY_COLOR[state], fontSize: typography.size.md };
}

export const textDisplayLabelStyle: TextStyle = {
  color: colors.textMuted,
  fontSize: typography.size.sm,
};

export const textDisplayHelperStyle: TextStyle = {
  color: colors.textMuted,
  fontSize: typography.size.sm,
};

export const screenStyles = {
  centered: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    gap: spacing.xs,
    backgroundColor: colors.bgApp,
  } satisfies ViewStyle,
  page: { backgroundColor: colors.bgApp } satisfies ViewStyle,
  pageContent: { padding: spacing.sm, gap: spacing.sm } satisfies ViewStyle,
  bodyText: {
    color: colors.textMain,
    fontSize: typography.size.md,
  } satisfies TextStyle,
  mutedText: {
    color: colors.textMuted,
    fontSize: typography.size.md,
  } satisfies TextStyle,
  metricRow: {
    flexDirection: "row",
    justifyContent: "space-between",
    gap: spacing.sm,
  } satisfies ViewStyle,
  navigation: {
    header: { backgroundColor: colors.bgSurface } satisfies ViewStyle,
    headerTitle: { fontWeight: typography.weight.bold } satisfies TextStyle,
  },
} as const;
