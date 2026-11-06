import type { ReactNode } from "react";
import type { PressableProps, StyleProp, ViewStyle } from "react-native";
import type { ButtonSize, ButtonVariant } from "@aquarium/theme";

export type { ButtonSize, ButtonVariant };

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
