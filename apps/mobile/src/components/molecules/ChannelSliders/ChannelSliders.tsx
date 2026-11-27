import type { ChannelLevels } from "@aquarium/api-client";
import { cardStyles, spacing } from "@aquarium/theme";
import { CHANNEL_KEYS, channelLabels } from "@aquarium/view-model";
import { Text, View } from "react-native";
import { Slider } from "@/components/atoms";
import { useT } from "@/i18n/use-locale";
import type { ReactNode } from "react";

export interface ChannelSlidersProps {
  levels: ChannelLevels;
  onCommit: (levels: ChannelLevels) => void;
  disabled?: boolean;
}

export const ChannelSliders = ({
  levels,
  onCommit,
  disabled,
}: ChannelSlidersProps): ReactNode => {
  const t = useT();
  const labels = channelLabels(t);

  return (
    <View style={{ gap: spacing.xs }}>
      <Text style={cardStyles.sectionLabel}>
        {t("lighting.manualChannels")}
      </Text>
      {CHANNEL_KEYS.map((channel) => (
        <Slider
          key={channel}
          label={labels[channel]}
          value={levels[channel]}
          disabled={disabled ?? false}
          onCommit={(value) => onCommit({ ...levels, [channel]: value })}
        />
      ))}
    </View>
  );
};
