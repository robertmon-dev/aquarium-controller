import type { Control, FieldValues, Path } from "react-hook-form";
import type { KeyboardTypeOptions } from "react-native";

export interface FormFieldProps<
  T extends FieldValues,
  TContext = unknown,
  TTransformed = T,
> {
  control: Control<T, TContext, TTransformed>;
  name: Path<T>;
  label: string;
  placeholder?: string;
  keyboardType?: KeyboardTypeOptions;
  secureTextEntry?: boolean;
  helperText?: string;
}
