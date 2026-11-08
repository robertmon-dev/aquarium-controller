import type { Metric, StatusTone } from "@aquarium/view-model";
import type { ReactNode } from "react";
import { Card, StatusDot } from "@/components/atoms";
import { MetricList } from "@/components/molecules/MetricList/MetricList";
import "./MetricCard.scss";

export interface MetricCardProps {
  title: string;
  status: StatusTone;
  statusLabel: string;
  metrics?: Metric[];
  children?: ReactNode;
}

export function MetricCard({
  title,
  status,
  statusLabel,
  metrics,
  children,
}: MetricCardProps) {
  return (
    <Card variant="outlined" padding="md" width="full">
      <header className="metric-card__header">
        <h3 className="metric-card__title">{title}</h3>
        <StatusDot tone={status} label={statusLabel} />
      </header>

      {metrics ? <MetricList metrics={metrics} /> : null}
      {children}
    </Card>
  );
}
