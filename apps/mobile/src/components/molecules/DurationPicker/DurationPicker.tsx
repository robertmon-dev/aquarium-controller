import { cardStyles, spacing } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Chip } from "@/components/atoms";
import { durationChoices, type DurationChoice } from "@aquarium/view-model";
import { useT } from "@/i18n/use-locale";
import type { ReactNode } from "react";

export interface DurationPickerProps {
  selected: number | null;
  onSelect: (seconds: number | null) => void;
  choices?: DurationChoice[];
}

export const DurationPicker = ({
  selected,
  onSelect,
  choices,
}: DurationPickerProps): ReactNode => {
  const t = useT();

  return (
    <View style={{ gap: spacing.xs }}>
      <Text style={cardStyles.sectionLabel}>{t("lighting.duration")}</Text>
      <View style={cardStyles.chipRow}>
        {(choices ?? durationChoices(t)).map((choice) => (
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
};
