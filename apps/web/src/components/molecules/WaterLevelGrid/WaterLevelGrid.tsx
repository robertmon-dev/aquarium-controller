import type { ChipVariant, WaterLevelChip } from "@aquarium/view-model";
import { Tag } from "@/components/atoms";
import type { TagVariant } from "@/components/atoms/Tag/types";
import "./WaterLevelGrid.scss";

const TAG_VARIANT: Record<ChipVariant, TagVariant> = {
  default: "default",
  wet: "info",
  dry: "default",
  faulty: "danger",
};

export function WaterLevelGrid({ chips }: { chips: WaterLevelChip[] }) {
  return (
    <div className="water-grid">
      {chips.map((chip) => (
        <Tag key={chip.label} variant={TAG_VARIANT[chip.variant]} size="sm">
          {chip.label}
        </Tag>
      ))}
    </div>
  );
}
