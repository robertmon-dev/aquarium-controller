import type { StyleProp, ViewStyle } from "react-native";
import type { ChipVariant } from "@aquarium/theme";

export type { ChipVariant };

export interface ChipProps {
  label: string;
  variant?: ChipVariant;
  selected?: boolean;
  onPress?: () => void;
  style?: StyleProp<ViewStyle>;
}
