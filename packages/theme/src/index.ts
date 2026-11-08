import type { TextStyle, ViewStyle } from "react-native";
import { withAlpha } from "./color";
import type {
  BadgeVariant,
  ChipVariant,
  StatusTone,
  TextDisplayState,
} from "@aquarium/view-model/semantics";
export { withAlpha } from "./color";
export type {
  BadgeVariant,
  ChipVariant,
  StatusTone,
  TextDisplayState,
} from "@aquarium/view-model/semantics";

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

  bgField: "#2c3148",
  border: withAlpha(palette.slate, 0.4),
} as const;

const TINT_FILL = 0.1;
const TINT_BORDER = 0.2;

export function tintedSurface(accent: string): ViewStyle {
  return {
    backgroundColor: withAlpha(accent, TINT_FILL),
    borderWidth: 1,
    borderColor: withAlpha(accent, TINT_BORDER),
  };
}

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
  weight: { regular: "400", medium: "500", semibold: "600", bold: "700" },
} as const;

const GLOW_RIM_TOP = {
  offsetX: 0,
  offsetY: 1,
  blurRadius: 0,
  spreadDistance: 0,
  color: withAlpha(palette.blue, 0.15),
  inset: true,
} as const;

export const shadows = {
  glowRimTop: [GLOW_RIM_TOP],
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
    GLOW_RIM_TOP,
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
    GLOW_RIM_TOP,
  ],
} as const;

function liftShadow(accent: string) {
  return [
    {
      offsetX: 0,
      offsetY: 4,
      blurRadius: 8,
      spreadDistance: -4,
      color: withAlpha(accent, 0.5),
    },
    GLOW_RIM_TOP,
  ];
}

export const theme = {
  palette,
  colors,
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

const BUTTON_PADDING: Record<ButtonSize, number> = { sm: 12, md: 20, lg: 28 };

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
      backgroundColor: colors.primaryHover,
      borderColor: withAlpha("#ffffff", 0.1),
      borderWidth: 1,
      boxShadow: liftShadow(colors.primary),
    },
    label: { color: "#ffffff" },
  },
  secondary: {
    container: {
      backgroundColor: colors.bgSurface,
      borderColor: withAlpha(colors.textMuted, 0.3),
      borderWidth: 2,
      boxShadow: [
        {
          offsetX: 0,
          offsetY: 2,
          blurRadius: 4,
          spreadDistance: 0,
          color: withAlpha("#000000", 0.2),
        },
      ],
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
      backgroundColor: withAlpha(colors.danger, 0.1),
      borderColor: withAlpha(colors.danger, 0.5),
      borderWidth: 2.5,
    },
    label: { color: colors.danger },
  },
  success: {
    container: {
      backgroundColor: colors.success,
      borderColor: withAlpha("#ffffff", 0.2),
      borderWidth: 1,
      boxShadow: liftShadow(colors.success),
    },
    label: { color: colors.textOnAccent },
  },
  purple: {
    container: {
      backgroundColor: palette.purple,
      borderColor: withAlpha("#ffffff", 0.1),
      borderWidth: 1,
      boxShadow: liftShadow(palette.purple),
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
  success: [colors.success, "#92c857"],
  purple: [palette.purple, "#9b6bf3"],
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
    paddingHorizontal: isIcon ? 0 : BUTTON_PADDING[size],
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
    fontWeight: typography.weight.semibold,
    ...BUTTON_VARIANTS[variant].label,
  };
}

export const inputStyles = {
  wrapper: { gap: spacing.xxs } satisfies ViewStyle,
  label: {
    color: colors.textMain,
    fontSize: typography.size.sm,
    fontWeight: typography.weight.medium,
    marginLeft: spacing.sm,
  } satisfies TextStyle,
  field: {
    flexDirection: "row",
    alignItems: "center",
    gap: spacing.xs,
    minHeight: 44,
    paddingHorizontal: 20,
    backgroundColor: colors.bgField,
    borderWidth: 2,
    borderColor: withAlpha(colors.primary, 0),
    borderRadius: radius.mdL,
  } satisfies ViewStyle,
  fieldFocused: {
    borderColor: withAlpha(colors.primary, 0.3),
  } satisfies ViewStyle,
  fieldError: { borderColor: colors.danger } satisfies ViewStyle,
  fieldErrorFocused: {
    borderColor: colors.danger,
    boxShadow: [
      {
        offsetX: 0,
        offsetY: 0,
        blurRadius: 0,
        spreadDistance: 1.56,
        color: withAlpha(colors.danger, 0.25),
      },
    ],
  } satisfies ViewStyle,
  control: {
    flex: 1,
    color: colors.textMain,
    fontSize: typography.size.md,
    paddingVertical: spacing.xs,
  } satisfies TextStyle,
  helper: {
    color: colors.textMuted,
    fontSize: typography.size.sm,
    marginLeft: radius.mdL,
  } satisfies TextStyle,
  error: {
    color: colors.danger,
    fontSize: typography.size.sm,
    fontWeight: typography.weight.medium,
    marginLeft: radius.mdL,
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
  elevated: {
    backgroundColor: colors.bgSurface,
    borderColor: withAlpha(palette.slate, 0.5),
    boxShadow: shadows.surfaceSm,
  },
  outlined: {
    backgroundColor: "transparent",
    borderColor: colors.border,
  },
  flat: {
    backgroundColor: colors.bgSidebar,
    borderColor: withAlpha(palette.slate, 0.3),
  },
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
    borderWidth: 2,
    borderColor: "transparent",
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

export function alertAccent(variant: AlertVariant): string {
  return ALERT_ACCENT[variant];
}

export function alertStyle(variant: AlertVariant): ViewStyle {
  const accent = ALERT_ACCENT[variant];

  return {
    flexDirection: "row",
    alignItems: "flex-start",
    gap: spacing.sm,
    minHeight: 48,
    paddingVertical: spacing.sm,
    paddingHorizontal: spacing.md,
    borderRadius: radius.md,
    ...tintedSurface(accent),
    boxShadow: [
      {
        offsetX: 0,
        offsetY: 0,
        blurRadius: 15,
        spreadDistance: 0,
        color: withAlpha(accent, 0.05),
      },
    ],
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
  fontSize: typography.size.sm,
  opacity: 0.85,
};

export const alertCloseStyle: TextStyle = {
  color: colors.textMuted,
  fontSize: typography.size.lg,
};

export type BadgeSize = "sm" | "md";

const BADGE_VARIANTS: Record<
  BadgeVariant,
  { container: ViewStyle; label: TextStyle }
> = {
  primary: {
    container: tintedSurface(colors.primary),
    label: { color: colors.primary },
  },
  secondary: {
    container: tintedSurface(colors.textMuted),
    label: { color: colors.textMuted },
  },
  warning: {
    container: tintedSurface(colors.warning),
    label: { color: colors.warning },
  },
  success: {
    container: tintedSurface(colors.success),
    label: { color: colors.success },
  },
  danger: {
    container: tintedSurface(colors.danger),
    label: { color: colors.danger },
  },
  info: {
    container: tintedSurface(colors.info),
    label: { color: colors.info },
  },
  outline: {
    container: {
      backgroundColor: "transparent",
      borderWidth: 1,
      borderColor: colors.border,
    },
    label: { color: colors.textMain },
  },
};

const BADGE_SIZES: Record<
  BadgeSize,
  {
    paddingVertical: number;
    paddingHorizontal: number;
    font: number;
    tracking: number;
  }
> = {
  sm: {
    paddingVertical: 2,
    paddingHorizontal: spacing.xs,
    font: typography.size.xs,
    tracking: 0.2,
  },
  md: {
    paddingVertical: 4,
    paddingHorizontal: spacing.sm,
    font: typography.size.sm,
    tracking: 0.4,
  },
};

export function badgeContainerStyle(
  variant: BadgeVariant,
  size: BadgeSize,
): ViewStyle {
  return {
    alignSelf: "flex-start",
    alignItems: "center",
    justifyContent: "center",
    paddingVertical: BADGE_SIZES[size].paddingVertical,
    paddingHorizontal: BADGE_SIZES[size].paddingHorizontal,
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
    fontWeight: typography.weight.semibold,
    letterSpacing: BADGE_SIZES[size].tracking,
    ...BADGE_VARIANTS[variant].label,
  };
}

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
  fontWeight: typography.weight.semibold,
  letterSpacing: 0.6,
  textTransform: "uppercase",
};

export const textDisplayHelperStyle: TextStyle = {
  color: colors.textMuted,
  fontSize: typography.size.sm,
};

function tagSurface(accent: string): ViewStyle {
  return {
    backgroundColor: withAlpha(accent, 0.15),
    borderColor: withAlpha(accent, 0.3),
    boxShadow: [
      {
        offsetX: 0,
        offsetY: 1,
        blurRadius: 0,
        spreadDistance: 0,
        color: withAlpha("#ffffff", 0.05),
        inset: true,
      },
    ],
  };
}

const CHIP_VARIANTS: Record<
  ChipVariant,
  { container: ViewStyle; label: TextStyle }
> = {
  default: {
    container: tagSurface(colors.textMuted),
    label: { color: colors.textMain },
  },
  wet: {
    container: tagSurface(colors.primary),
    label: { color: colors.primary },
  },
  dry: {
    container: { backgroundColor: "transparent", borderColor: colors.border },
    label: { color: colors.textMuted },
  },
  faulty: {
    container: tagSurface(colors.danger),
    label: { color: colors.danger },
  },
};

export function chipContainerStyle(
  variant: ChipVariant,
  selected: boolean,
): ViewStyle {
  return {
    alignItems: "center",
    justifyContent: "center",
    minHeight: 34,
    paddingHorizontal: spacing.sm,
    borderRadius: radius.mdL,
    borderWidth: selected ? 2 : 1,
    ...CHIP_VARIANTS[variant].container,
    ...(selected
      ? {
          backgroundColor: withAlpha(colors.primary, 0.15),
          borderColor: colors.primary,
        }
      : {}),
  };
}

export function chipLabelStyle(
  variant: ChipVariant,
  selected: boolean,
): TextStyle {
  return {
    fontSize: typography.size.sm,
    fontWeight: selected ? typography.weight.bold : typography.weight.medium,
    ...CHIP_VARIANTS[variant].label,
    ...(selected ? { color: colors.primary } : {}),
  };
}

const STATUS_COLOR: Record<StatusTone, string> = {
  ok: colors.success,
  degraded: colors.warning,
  off: colors.textMuted,
};

export function statusDotStyle(tone: StatusTone): ViewStyle {
  return {
    width: 8,
    height: 8,
    borderRadius: 4,
    backgroundColor: STATUS_COLOR[tone],
  };
}

export function statusLabelStyle(tone: StatusTone): TextStyle {
  return { color: STATUS_COLOR[tone], fontSize: typography.size.sm };
}

export const sliderStyles = {
  track: { width: "100%", height: 40 } satisfies ViewStyle,
  minimumTrackTintColor: colors.primary,
  maximumTrackTintColor: colors.border,
  thumbTintColor: colors.primary,
  label: {
    flexDirection: "row",
    justifyContent: "space-between",
  } satisfies ViewStyle,
} as const;

export const cardStyles = {
  header: {
    flexDirection: "row",
    alignItems: "center",
    justifyContent: "space-between",
    marginBottom: spacing.xs,
  } satisfies ViewStyle,
  title: {
    color: colors.textMain,
    fontSize: typography.size.md,
    fontWeight: typography.weight.bold,
  } satisfies TextStyle,
  status: {
    flexDirection: "row",
    alignItems: "center",
    gap: spacing.xxs * 2,
  } satisfies ViewStyle,
  heroValue: {
    color: colors.textMain,
    fontSize: 44,
    fontWeight: typography.weight.bold,
  } satisfies TextStyle,
  heroUnit: {
    color: colors.textMuted,
    fontSize: typography.size.lg,
  } satisfies TextStyle,
  heroRow: {
    flexDirection: "row",
    alignItems: "baseline",
    gap: spacing.xs,
  } satisfies ViewStyle,
  chipRow: {
    flexDirection: "row",
    flexWrap: "wrap",
    gap: spacing.xs,
  } satisfies ViewStyle,
  sectionLabel: {
    color: colors.textMuted,
    fontSize: typography.size.sm,
    fontWeight: typography.weight.medium,
  } satisfies TextStyle,
} as const;

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
    header: {
      backgroundColor: colors.bgSurface,
      borderBottomWidth: 1,
      borderBottomColor: colors.border,
      boxShadow: "none",
    } satisfies ViewStyle,
    tabBar: {
      backgroundColor: colors.bgSurface,
      borderTopWidth: 1,
      borderTopColor: colors.border,
      boxShadow: "none",
    } satisfies ViewStyle,
    headerTitle: { fontWeight: typography.weight.bold } satisfies TextStyle,
  },
} as const;
