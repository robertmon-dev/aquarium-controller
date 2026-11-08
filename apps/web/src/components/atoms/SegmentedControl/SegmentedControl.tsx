import { Button } from "@/components/atoms/Button/Button";
import type {
  ButtonSize,
  ButtonVariant,
} from "@/components/atoms/Button/types";
import "./SegmentedControl.scss";

export interface Segment<T extends string> {
  value: T;
  label: string;
}

export interface SegmentedControlProps<T extends string> {
  segments: readonly Segment<T>[];
  selected: T;
  onSelect: (value: T) => void;
  ariaLabel: string;
  variant?: ButtonVariant;
  inactiveVariant?: ButtonVariant;
  size?: ButtonSize;
}

export function SegmentedControl<T extends string>({
  segments,
  selected,
  onSelect,
  ariaLabel,
  variant = "primary",
  inactiveVariant = "outline",
  size = "md",
}: SegmentedControlProps<T>) {
  return (
    <div className="segmented" role="group" aria-label={ariaLabel}>
      {segments.map((segment) => {
        const active = segment.value === selected;

        return (
          <Button
            key={segment.value}
            type="button"
            size={size}
            variant={active ? variant : inactiveVariant}
            aria-pressed={active}
            className={
              active
                ? "segmented__item segmented__item--active"
                : "segmented__item"
            }
            onClick={() => onSelect(segment.value)}
          >
            {segment.label}
          </Button>
        );
      })}
    </div>
  );
}
