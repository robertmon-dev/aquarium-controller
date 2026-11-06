import { useTelemetry, type Transport } from "@aquarium/api-client";
import { View } from "react-native";
import { Alert, Badge, Card } from "@/components/atoms";
import { MetricList, ScreenMessage } from "@/components/molecules";
import { spacing } from "@/components/utility/theme";
import { telemetryMetrics } from "@/components/utility/telemetry-metrics";
import type { StreamState } from "@/device/use-telemetry-stream";
import { streamBadge } from "./stream-badge";

export interface TelemetryPanelProps {
  transport: Transport;
  streamState: StreamState;
}

export function TelemetryPanel({
  transport,
  streamState,
}: TelemetryPanelProps) {
  const { data, error, isPending } = useTelemetry(transport);
  const badge = streamBadge(streamState);

  if (isPending)
    return <ScreenMessage loading>fetching snapshot…</ScreenMessage>;
  if (error) {
    return (
      <Alert variant="danger" title="telemetry unavailable">
        {error.message}
      </Alert>
    );
  }
  if (!data) return <ScreenMessage>no data</ScreenMessage>;

  return (
    <Card variant="elevated" padding="md" width="full">
      <View style={{ gap: spacing.sm }}>
        <Badge variant={badge.variant}>{badge.label}</Badge>
        <MetricList metrics={telemetryMetrics(data)} />
      </View>
    </Card>
  );
}
