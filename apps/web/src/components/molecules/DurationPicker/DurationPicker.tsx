import { useT } from "@aquarium/i18n/use-locale";
import { durationChoices } from "@aquarium/view-model";
import { Button } from "@/components/atoms";
import type { ReactNode } from "react";

export const DurationPicker = ({
  selected,
  onSelect,
}: {
  selected: number | null;
  onSelect: (seconds: number | null) => void;
}): ReactNode => {
  const t = useT();

  return (
    <div className="picker">
      {durationChoices(t).map((choice) => (
        <Button
          key={choice.label}
          variant={choice.seconds === selected ? "primary" : "outline"}
          onClick={() => onSelect(choice.seconds)}
        >
          {choice.label}
        </Button>
      ))}
    </div>
  );
};
