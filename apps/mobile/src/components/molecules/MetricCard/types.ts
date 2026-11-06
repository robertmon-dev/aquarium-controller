import type { ReactNode } from "react";
import type { StatusTone } from "@aquarium/theme";
import type { Metric } from "@/components/molecules/MetricList/types";

export interface MetricCardProps {
  title: string;
  status: StatusTone;
  statusLabel: string;
  metrics?: Metric[];
  children?: ReactNode;
}
