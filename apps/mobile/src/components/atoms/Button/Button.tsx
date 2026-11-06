import {
  buttonContainerStyle,
  buttonGradient,
  buttonLabelStyle,
} from "@aquarium/theme";
import { LinearGradient } from "expo-linear-gradient";
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  View,
} from "react-native";
import type { ButtonProps } from "./types";

export type { ButtonProps, ButtonSize, ButtonVariant } from "./types";

export function Button({
  variant = "primary",
  size = "md",
  isLoading = false,
  fullWidth = false,
  isIcon = false,
  leftIcon,
  rightIcon,
  children,
  disabled,
  style,
  ...rest
}: ButtonProps) {
  const inert = disabled === true || isLoading;
  const labelStyle = buttonLabelStyle(variant, size);
  const gradient = buttonGradient(variant);

  return (
    <Pressable
      accessibilityRole="button"
      disabled={inert}
      style={({ pressed }) => [
        buttonContainerStyle({
          variant,
          size,
          fullWidth,
          isIcon,
          pressed,
          disabled: inert,
        }),
        style,
      ]}
      {...rest}
    >
      {gradient ? (
        <LinearGradient
          colors={gradient}
          start={{ x: 0, y: 0 }}
          end={{ x: 1, y: 1 }}
          style={StyleSheet.absoluteFill}
        />
      ) : null}

      {isLoading ? (
        <ActivityIndicator size="small" color={labelStyle.color} />
      ) : (
        <>
          {leftIcon ? <View>{leftIcon}</View> : null}
          {typeof children === "string" ? (
            <Text style={labelStyle}>{children}</Text>
          ) : (
            children
          )}
          {rightIcon ? <View>{rightIcon}</View> : null}
        </>
      )}
    </Pressable>
  );
}
