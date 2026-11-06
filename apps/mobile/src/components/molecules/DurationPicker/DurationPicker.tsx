import { cardStyles, spacing } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Chip } from "@/components/atoms";
import { DURATION_CHOICES } from "@/components/utility/lighting-request";

export interface DurationPickerProps {
  selected: number | null;
  onSelect: (seconds: number | null) => void;
}

export function DurationPicker({ selected, onSelect }: DurationPickerProps) {
  return (
    <View style={{ gap: spacing.xs }}>
      <Text style={cardStyles.sectionLabel}>duration</Text>
      <View style={cardStyles.chipRow}>
        {DURATION_CHOICES.map((choice) => (
          <Chip
            key={choice.label}
            label={choice.label}
            selected={choice.seconds === selected}
            onPress={() => onSelect(choice.seconds)}
          />
        ))}
      </View>
    </View>
  );
}
