import type { ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";

export type TextDisplayState =
  | "default"
  | "error"
  | "success"
  | "warning"
  | "info";

export interface TextDisplayProps {
  label?: string;
  children: ReactNode;
  state?: TextDisplayState;
  helperText?: string;
  fullWidth?: boolean;
  maxHeight?: number;
  style?: StyleProp<ViewStyle>;
}
