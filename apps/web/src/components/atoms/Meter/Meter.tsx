import "./Meter.scss";

export interface MeterProps {
  value: number;
  label: string;
  valueText: string;
}

export function Meter({ value, label, valueText }: MeterProps) {
  const clamped = Math.min(1, Math.max(0, value));

  return (
    <div className="meter">
      <div className="meter__row">
        <span className="meter__label">{label}</span>
        <span className="meter__value">{valueText}</span>
      </div>
      <div
        className="meter__track"
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(clamped * 100)}
        aria-valuetext={valueText}
      >
        <div className="meter__fill" style={{ width: `${clamped * 100}%` }} />
      </div>
    </div>
  );
}
