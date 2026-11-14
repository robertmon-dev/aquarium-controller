import {
  alertBodyStyle,
  alertCloseStyle,
  alertStyle,
  alertTitleStyle,
  spacing,
} from "@aquarium/theme";
import { Pressable, Text, View } from "react-native";
import type { AlertProps } from "./types";
import type { ReactNode } from "react";

export type { AlertProps, AlertVariant } from "./types";

export const Alert = ({
  variant = "info",
  title,
  children,
  onClose,
  action,
  style,
}: AlertProps): ReactNode => {
  return (
    <View style={[alertStyle(variant), style]}>
      <View style={{ flex: 1, gap: spacing.xxs }}>
        {title ? <Text style={alertTitleStyle(variant)}>{title}</Text> : null}
        {typeof children === "string" ? (
          <Text style={alertBodyStyle}>{children}</Text>
        ) : (
          children
        )}
      </View>

      {action ? <View>{action}</View> : null}

      {onClose ? (
        <Pressable accessibilityRole="button" onPress={onClose} hitSlop={8}>
          <Text style={alertCloseStyle}>×</Text>
        </Pressable>
      ) : null}
    </View>
  );
};
