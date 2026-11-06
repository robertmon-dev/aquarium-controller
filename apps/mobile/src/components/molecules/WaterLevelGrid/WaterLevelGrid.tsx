import { cardStyles } from "@aquarium/theme";
import { View } from "react-native";
import { Chip } from "@/components/atoms";
import type { WaterLevelChip } from "@/components/utility/water-level";

export interface WaterLevelGridProps {
  chips: WaterLevelChip[];
}

export function WaterLevelGrid({ chips }: WaterLevelGridProps) {
  return (
    <View style={cardStyles.chipRow}>
      {chips.map((chip) => (
        <Chip key={chip.label} label={chip.label} variant={chip.variant} />
      ))}
    </View>
  );
}
