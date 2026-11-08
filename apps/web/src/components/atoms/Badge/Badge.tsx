import type { BadgeProps } from "./types";
import "./Badge.scss";

export const Badge = ({
  variant = "primary",
  children,
  className = "",
  size = "sm",
}: BadgeProps) => {
  return (
    <span
      className={`badge badge--${variant} badge--size-${size} ${className}`}
    >
      {children}
    </span>
  );
};
