import { screenStyles } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Spinner } from "@/components/atoms";
import type { ScreenMessageProps } from "./types";
import type { ReactNode } from "react";

export type { ScreenMessageProps } from "./types";

export const ScreenMessage = ({
  children,
  loading = false,
}: ScreenMessageProps): ReactNode => {
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
};
