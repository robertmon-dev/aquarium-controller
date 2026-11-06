import type { ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";

export type AlertVariant = "info" | "success" | "warning" | "danger";

export interface AlertProps {
  variant?: AlertVariant;
  title?: string;
  children: ReactNode;
  onClose?: () => void;
  style?: StyleProp<ViewStyle>;
}
