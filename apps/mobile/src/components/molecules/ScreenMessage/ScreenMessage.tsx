import { screenStyles } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Spinner } from "@/components/atoms";
import type { ScreenMessageProps } from "./types";

export type { ScreenMessageProps } from "./types";

export function ScreenMessage({
  children,
  loading = false,
}: ScreenMessageProps) {
  return (
    <View style={screenStyles.centered}>
      {loading ? <Spinner /> : null}
      {typeof children === "string" ? (
        <Text style={screenStyles.bodyText}>{children}</Text>
      ) : (
        children
      )}
    </View>
  );
}
