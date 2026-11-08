import { bem } from "@/components/utility/bem";
import "./Spinner.scss";

export type SpinnerSize = "sm" | "md" | "lg";

export interface SpinnerProps {
  size?: SpinnerSize;
  className?: string;
  label?: string;
}

export function Spinner({ size = "md", className, label }: SpinnerProps) {
  return (
    <span
      className={bem("spinner", [size], className)}
      role="status"
      aria-label={label ?? "loading"}
    />
  );
}
