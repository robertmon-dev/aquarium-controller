import type { LightingProfile } from "@aquarium/api-client";
import { cardStyles, spacing } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Chip } from "@/components/atoms";
import { LIGHTING_PROFILES } from "@/components/utility/lighting-request";
import { useT } from "@/i18n/use-locale";

export interface ProfilePickerProps {
  selected: LightingProfile | null;
  onSelect: (profile: LightingProfile) => void;
  disabled?: boolean;
}

export function ProfilePicker({
  selected,
  onSelect,
  disabled,
}: ProfilePickerProps) {
  const t = useT();

  return (
    <View style={{ gap: spacing.xs }}>
      <Text style={cardStyles.sectionLabel}>{t("lighting.profile")}</Text>
      <View style={cardStyles.chipRow}>
        {LIGHTING_PROFILES.map((profile) => (
          <Chip
            key={profile}
            label={t(`profiles.${profile}`)}
            selected={profile === selected}
            {...(disabled ? {} : { onPress: () => onSelect(profile) })}
          />
        ))}
      </View>
    </View>
  );
}
