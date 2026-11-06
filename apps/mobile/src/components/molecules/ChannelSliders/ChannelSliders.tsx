import type { ChannelLevels } from "@aquarium/api-client";
import { cardStyles, spacing } from "@aquarium/theme";
import { Text, View } from "react-native";
import { Slider } from "@/components/atoms";

export interface ChannelSlidersProps {
  levels: ChannelLevels;
  onCommit: (levels: ChannelLevels) => void;
  disabled?: boolean;
}

export function ChannelSliders({
  levels,
  onCommit,
  disabled,
}: ChannelSlidersProps) {
  return (
    <View style={{ gap: spacing.xs }}>
      <Text style={cardStyles.sectionLabel}>manual channels</Text>
      <Slider
        label="white"
        value={levels.white_pct}
        disabled={disabled ?? false}
        onCommit={(white_pct) => onCommit({ ...levels, white_pct })}
      />
      <Slider
        label="rgb"
        value={levels.rgb_pct}
        disabled={disabled ?? false}
        onCommit={(rgb_pct) => onCommit({ ...levels, rgb_pct })}
      />
    </View>
  );
}
