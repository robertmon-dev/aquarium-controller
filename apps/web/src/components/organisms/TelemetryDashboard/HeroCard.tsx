import type { HeroCardData } from "@aquarium/view-model";
import { Card, StatusDot } from "@/components/atoms";
import { MetricList } from "@/components/molecules";
import "./HeroCard.scss";

export function HeroCard({ card }: { card: HeroCardData }) {
  return (
    <Card variant="levitating" padding="lg" width="full">
      <header className="hero__header">
        <h2 className="hero__title">{card.title}</h2>
        <StatusDot tone={card.status} label={card.statusLabel} />
      </header>

      <div className="hero__row">
        <span className="hero__value">{card.heroValue}</span>
        <span className="hero__unit">{card.heroUnit}</span>
      </div>

      <MetricList metrics={card.metrics} />
    </Card>
  );
}
