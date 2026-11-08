import { colors, screenStyles, spacing, typography } from "@aquarium/theme";
import { Ionicons } from "@expo/vector-icons";
import { Text, View } from "react-native";
import { Button } from "@/components/atoms";
import type { DeviceNoticeProps } from "./types";

export type { DeviceNoticeProps } from "./types";

export function DeviceNotice({
  icon,
  title,
  message,
  actionLabel,
  onAction,
}: DeviceNoticeProps) {
  return (
    <View style={[screenStyles.centered, { gap: spacing.md }]}>
      <Ionicons name={icon} size={64} color={colors.primary} />

      <Text
        style={{
          color: colors.textMain,
          fontFamily: typography.fontFamily,
          fontSize: typography.size.lg,
          fontWeight: typography.weight.semibold,
          textAlign: "center",
        }}
      >
        {title}
      </Text>

      <Text style={[screenStyles.mutedText, { textAlign: "center" }]}>
        {message}
      </Text>

      {actionLabel && onAction ? (
        <Button variant="primary" onPress={onAction}>
          {actionLabel}
        </Button>
      ) : null}
    </View>
  );
}
