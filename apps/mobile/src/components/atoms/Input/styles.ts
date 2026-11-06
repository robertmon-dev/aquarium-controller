import { StyleSheet } from "react-native";
import {
  colors,
  radius,
  spacing,
  typography,
} from "@/components/utility/theme";

export const inputStyles = StyleSheet.create({
  wrapper: { gap: spacing.xs / 2 },
  label: {
    color: colors.textMain,
    fontSize: typography.size.sm,
    fontWeight: typography.weight.medium,
  },
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
  },
  fieldFocused: { borderColor: colors.primary },
  fieldError: { borderColor: colors.danger },
  control: {
    flex: 1,
    color: colors.textMain,
    fontSize: typography.size.md,
    paddingVertical: spacing.xs,
  },
  helper: { color: colors.textMuted, fontSize: typography.size.sm },
  error: { color: colors.danger, fontSize: typography.size.sm },
});
