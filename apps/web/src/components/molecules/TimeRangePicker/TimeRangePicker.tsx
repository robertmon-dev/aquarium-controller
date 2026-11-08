import { useT } from "@aquarium/i18n/use-locale";
import { Button } from "@/components/atoms";
import {
  RANGES,
  type RangeKey,
} from "@/components/organisms/HistoryPanel/history-reducer";

export function TimeRangePicker({
  selected,
  onSelect,
}: {
  selected: RangeKey;
  onSelect: (key: RangeKey) => void;
}) {
  const t = useT();

  return (
    <div className="picker">
      {(Object.keys(RANGES) as RangeKey[]).map((key) => (
        <Button
          key={key}
          variant={key === selected ? "primary" : "outline"}
          onClick={() => onSelect(key)}
        >
          {t(`history.ranges.${key}`)}
        </Button>
      ))}
    </div>
  );
}
