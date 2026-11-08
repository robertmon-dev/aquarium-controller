import type { StyleProp, ViewStyle } from "react-native";
import type { Metric } from "@aquarium/view-model";

export type { Metric };

export interface MetricListProps {
  metrics: Metric[];
  style?: StyleProp<ViewStyle>;
}
