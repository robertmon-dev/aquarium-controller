import type { ReactNode } from "react";
import type { StyleProp, ViewStyle } from "react-native";
import type { TextDisplayState } from "@aquarium/theme";

export type { TextDisplayState };

export interface TextDisplayProps {
  label?: string;
  children: ReactNode;
  state?: TextDisplayState;
  helperText?: string;
  fullWidth?: boolean;
  maxHeight?: number;
  style?: StyleProp<ViewStyle>;
}
