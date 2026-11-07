import { cardStyles, spacing } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Chip } from "@/components/atoms";
import { durationChoices } from "@/components/utility/lighting-request";
import { useT } from "@/i18n/use-locale";

export interface DurationPickerProps {
  selected: number | null;
  onSelect: (seconds: number | null) => void;
}

export function DurationPicker({ selected, onSelect }: DurationPickerProps) {
  const t = useT();

  return (
    <View style={{ gap: spacing.xs }}>
      <Text style={cardStyles.sectionLabel}>{t("lighting.duration")}</Text>
      <View style={cardStyles.chipRow}>
        {durationChoices(t).map((choice) => (
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
