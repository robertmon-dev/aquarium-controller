import type { ReactNode } from "react";
import "./Badge.scss";

export type BadgeVariant =
  | "primary"
  | "secondary"
  | "warning"
  | "success"
  | "danger"
  | "info"
  | "outline";

export interface BadgeProps {
  variant?: BadgeVariant;
  children: ReactNode;
  className?: string;
  size?: "sm" | "md";
}
