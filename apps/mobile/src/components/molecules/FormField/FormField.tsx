import { Controller, type FieldValues } from "react-hook-form";
import { Input } from "@/components/atoms";
import type { FormFieldProps } from "./types";

export type { FormFieldProps } from "./types";

export function FormField<
  T extends FieldValues,
  TContext = unknown,
  TTransformed = T,
>({
  control,
  name,
  label,
  placeholder,
  keyboardType,
  secureTextEntry,
  helperText,
}: FormFieldProps<T, TContext, TTransformed>) {
  return (
    <Controller
      control={control}
      name={name}
      render={({ field, fieldState }) => (
        <Input
          label={label}
          value={String(field.value ?? "")}
          onChangeText={field.onChange}
          onBlur={field.onBlur}
          fullWidth
          {...(fieldState.error?.message
            ? { error: fieldState.error.message }
            : {})}
          {...(helperText ? { helperText } : {})}
          {...(placeholder ? { placeholder } : {})}
          {...(keyboardType ? { keyboardType } : {})}
          {...(secureTextEntry ? { secureTextEntry } : {})}
        />
      )}
    />
  );
}
