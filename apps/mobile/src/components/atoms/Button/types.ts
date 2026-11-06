import type { ReactNode } from "react";
import type { PressableProps, StyleProp, ViewStyle } from "react-native";

export type ButtonVariant =
  | "primary"
  | "secondary"
  | "outline"
  | "ghost"
  | "danger"
  | "success"
  | "purple"
  | "select";

export type ButtonSize = "sm" | "md" | "lg";

export interface ButtonProps extends Omit<
  PressableProps,
  "children" | "style"
> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  isLoading?: boolean;
  children?: ReactNode;
  fullWidth?: boolean;
  leftIcon?: ReactNode;
  rightIcon?: ReactNode;
  isIcon?: boolean;
  style?: StyleProp<ViewStyle>;
}
