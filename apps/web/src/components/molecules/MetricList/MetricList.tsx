import type { Metric } from "@aquarium/view-model";
import { bem } from "@/components/utility/bem";
import "./MetricList.scss";

export function MetricList({ metrics }: { metrics: Metric[] }) {
  return (
    <dl className="metrics">
      {metrics.map((metric) => (
        <div className="metrics__row" key={metric.label}>
          <dt className="metrics__label">{metric.label}</dt>
          <dd className={bem("metrics__value", [metric.state ?? "default"])}>
            {metric.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}
