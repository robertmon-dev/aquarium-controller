import { colors, inputStyles } from "@aquarium/theme";
import { useState } from "react";
import { Text, TextInput, View } from "react-native";
import type { InputProps } from "./types";

export type { InputProps } from "./types";

export function Input({
  label,
  error,
  helperText,
  leftIcon,
  rightIcon,
  fullWidth = false,
  style,
  onFocus,
  onBlur,
  ...rest
}: InputProps) {
  const [focused, setFocused] = useState(false);
  const errorMessage = typeof error === "string" ? error : undefined;
  const hasError = error === true || errorMessage !== undefined;

  return (
    <View
      style={[inputStyles.wrapper, fullWidth ? { width: "100%" } : null, style]}
    >
      {label ? <Text style={inputStyles.label}>{label}</Text> : null}

      <View
        style={[
          inputStyles.field,
          focused ? inputStyles.fieldFocused : null,
          hasError ? inputStyles.fieldError : null,
          hasError && focused ? inputStyles.fieldErrorFocused : null,
        ]}
      >
        {leftIcon}
        <TextInput
          style={inputStyles.control}
          placeholderTextColor={colors.textMuted}
          autoCapitalize="none"
          autoCorrect={false}
          onFocus={(event) => {
            setFocused(true);
            onFocus?.(event);
          }}
          onBlur={(event) => {
            setFocused(false);
            onBlur?.(event);
          }}
          {...rest}
        />
        {rightIcon}
      </View>

      {errorMessage ? (
        <Text style={inputStyles.error}>{errorMessage}</Text>
      ) : helperText ? (
        <Text style={inputStyles.helper}>{helperText}</Text>
      ) : null}
    </View>
  );
}
