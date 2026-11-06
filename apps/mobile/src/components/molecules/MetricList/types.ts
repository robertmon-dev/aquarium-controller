import type { StyleProp, ViewStyle } from "react-native";
import type { TextDisplayState } from "@aquarium/theme";

export type Metric = {
  label: string;
  value: string;
  state?: TextDisplayState;
};

export interface MetricListProps {
  metrics: Metric[];
  style?: StyleProp<ViewStyle>;
}
