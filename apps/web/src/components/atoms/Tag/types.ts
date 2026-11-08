import type { ReactNode, HTMLAttributes } from "react";

export type TagVariant =
  | "default"
  | "primary"
  | "success"
  | "warning"
  | "danger"
  | "info";
export type TagSize = "sm" | "md" | "lg";

export interface TagProps extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
  variant?: TagVariant;
  size?: TagSize;
  icon?: ReactNode;
  onDismiss?: () => void;
  clickable?: boolean;
  disabled?: boolean;
  maxLength?: number;
}
