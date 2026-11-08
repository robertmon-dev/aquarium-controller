import type { ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";
import type { AlertVariant } from "@aquarium/theme";

export type { AlertVariant };

export interface AlertProps {
  variant?: AlertVariant;
  title?: string;
  children: ReactNode;
  onClose?: () => void;
  action?: ReactNode;
  style?: StyleProp<ViewStyle>;
}
