import type { ReactNode } from "react";
import "./MetricGrid.scss";

export function MetricGrid({ children }: { children: ReactNode }) {
  return <div className="metric-grid">{children}</div>;
}
