import "./SegmentedControl.scss";

export interface Segment<T extends string> {
  value: T;
  label: string;
}

export function SegmentedControl<T extends string>({
  segments,
  selected,
  onSelect,
  ariaLabel,
}: {
  segments: readonly Segment<T>[];
  selected: T;
  onSelect: (value: T) => void;
  ariaLabel: string;
}) {
  return (
    <div className="segmented" role="group" aria-label={ariaLabel}>
      {segments.map((segment) => (
        <button
          key={segment.value}
          type="button"
          className={
            segment.value === selected
              ? "segmented__item segmented__item--active"
              : "segmented__item"
          }
          aria-pressed={segment.value === selected}
          onClick={() => onSelect(segment.value)}
        >
          {segment.label}
        </button>
      ))}
    </div>
  );
}
