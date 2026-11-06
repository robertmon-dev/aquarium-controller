import { cardStyles, sliderStyles } from "@aquarium/theme";
import CommunitySlider from "@react-native-community/slider";
import { useEffect, useState } from "react";
import { Text, View } from "react-native";
import { formatPercent } from "@/components/utility/format";
import type { SliderProps } from "./types";

export type { SliderProps } from "./types";

export function Slider({
  label,
  value,
  onCommit,
  minimumValue = 0,
  maximumValue = 100,
  step = 1,
  disabled = false,
}: SliderProps) {
  const [dragging, setDragging] = useState(value);

  useEffect(() => {
    setDragging(value);
  }, [value]);

  return (
    <View>
      <View style={sliderStyles.label}>
        <Text style={cardStyles.sectionLabel}>{label}</Text>
        <Text style={cardStyles.sectionLabel}>
          {formatPercent(Math.round(dragging))}
        </Text>
      </View>
      <CommunitySlider
        style={sliderStyles.track}
        value={value}
        minimumValue={minimumValue}
        maximumValue={maximumValue}
        step={step}
        disabled={disabled}
        minimumTrackTintColor={sliderStyles.minimumTrackTintColor}
        maximumTrackTintColor={sliderStyles.maximumTrackTintColor}
        thumbTintColor={sliderStyles.thumbTintColor}
        onValueChange={setDragging}
        onSlidingComplete={onCommit}
      />
    </View>
  );
}
