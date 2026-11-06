import { Text, View } from "react-native";
import { Spinner } from "@/components/atoms";
import { colors, spacing, typography } from "@/components/utility/theme";
import type { ScreenMessageProps } from "./types";

export type { ScreenMessageProps } from "./types";

export function ScreenMessage({
  children,
  loading = false,
}: ScreenMessageProps) {
  return (
    <View
      style={{
        flex: 1,
        alignItems: "center",
        justifyContent: "center",
        gap: spacing.xs,
        backgroundColor: colors.bgApp,
      }}
    >
      {loading ? <Spinner /> : null}
      {typeof children === "string" ? (
        <Text style={{ color: colors.textMain, fontSize: typography.size.md }}>
          {children}
        </Text>
      ) : (
        children
      )}
    </View>
  );
}
