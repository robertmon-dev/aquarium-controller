import { Pressable, Text, View } from "react-native";
import { colors, typography } from "@/components/utility/theme";
import { alertAccent, alertStyle } from "./styles";
import type { AlertProps } from "./types";

export type { AlertProps, AlertVariant } from "./types";

export function Alert({
  variant = "info",
  title,
  children,
  onClose,
  style,
}: AlertProps) {
  return (
    <View style={[alertStyle(variant), style]}>
      <View style={{ flex: 1, gap: 2 }}>
        {title ? (
          <Text
            style={{
              color: alertAccent(variant),
              fontSize: typography.size.md,
              fontWeight: typography.weight.bold,
            }}
          >
            {title}
          </Text>
        ) : null}
        {typeof children === "string" ? (
          <Text
            style={{ color: colors.textMain, fontSize: typography.size.md }}
          >
            {children}
          </Text>
        ) : (
          children
        )}
      </View>

      {onClose ? (
        <Pressable accessibilityRole="button" onPress={onClose} hitSlop={8}>
          <Text
            style={{ color: colors.textMuted, fontSize: typography.size.lg }}
          >
            ×
          </Text>
        </Pressable>
      ) : null}
    </View>
  );
}
