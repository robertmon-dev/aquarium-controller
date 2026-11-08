import type { ReactNode } from "react";
import { Card } from "@/components/atoms";
import "./HistoryPanel.scss";

export interface CardReading {
  label: string;
  value: string;
}

export interface CardLegendItem {
  label: string;
  color: string;
  isReference: boolean;
}

export function HistoryCard({
  title,
  readings,
  legend,
  children,
}: {
  title: string;
  readings: CardReading[];
  legend: CardLegendItem[];
  children: ReactNode;
}) {
  return (
    <Card variant="outlined" padding="md" width="full">
      <header className="history-card__header">
        <div className="history-card__identity">
          <h3 className="history-card__title">{title}</h3>

          {readings.length > 0 ? (
            <p className="history-card__readings">
              {readings.map((reading, index) => (
                <span className="history-card__reading" key={reading.label}>
                  {index > 0 ? (
                    <span className="history-card__divider">|</span>
                  ) : null}
                  <strong className="history-card__value">
                    {reading.value}
                  </strong>
                  <span className="history-card__reading-label">
                    {reading.label}
                  </span>
                </span>
              ))}
            </p>
          ) : null}
        </div>

        <ul className="history-card__legend">
          {legend.map((item) => (
            <li className="history-card__legend-item" key={item.label}>
              <span
                className={
                  item.isReference
                    ? "history-card__swatch history-card__swatch--reference"
                    : "history-card__swatch"
                }
                style={{ color: item.color }}
                aria-hidden="true"
              />
              {item.label}
            </li>
          ))}
        </ul>
      </header>

      <div className="history-card__body">{children}</div>
    </Card>
  );
}
