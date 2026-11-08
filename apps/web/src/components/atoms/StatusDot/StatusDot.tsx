import type { StatusTone } from "@aquarium/view-model";
import { bem } from "@/components/utility/bem";
import "./StatusDot.scss";

export interface StatusDotProps {
  tone: StatusTone;
  label?: string;
}

export function StatusDot({ tone, label }: StatusDotProps) {
  return (
    <span className="status">
      <span className={bem("status-dot", [tone])} aria-hidden="true" />
      {label ? (
        <span className={bem("status-label", [tone])}>{label}</span>
      ) : null}
    </span>
  );
}
