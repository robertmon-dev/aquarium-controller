import { ApiError, useTelemetry, type Transport } from "@aquarium/api-client";
import { colors, screenStyles, spacing } from "@aquarium/theme";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { RefreshControl, ScrollView } from "react-native";
import { Alert } from "@/components/atoms";
import { ScreenMessage } from "@/components/molecules";
import { TelemetryDashboard } from "@/components/organisms";
import { useDevice } from "@/components/utility/app-providers";

export function TelemetryPage() {
  const { transport } = useDevice();

  if (!transport) return <ScreenMessage>no device configured</ScreenMessage>;

  return <TelemetryContent transport={transport} />;
}

function TelemetryContent({ transport }: { transport: Transport }) {
  const queryClient = useQueryClient();
  const [refreshing, setRefreshing] = useState(false);
  const { data, error, isPending } = useTelemetry(transport);

  const onRefresh = useCallback(async () => {
    setRefreshing(true);
    await queryClient.invalidateQueries();
    setRefreshing(false);
  }, [queryClient]);

  if (isPending)
    return <ScreenMessage loading>fetching snapshot…</ScreenMessage>;

  return (
    <ScrollView
      style={screenStyles.page}
      contentContainerStyle={{ padding: spacing.sm, gap: spacing.sm }}
      refreshControl={
        <RefreshControl
          refreshing={refreshing}
          onRefresh={() => void onRefresh()}
          tintColor={colors.primary}
        />
      }
    >
      {error ? <TelemetryError error={error} /> : null}
      {data ? <TelemetryDashboard snapshot={data} /> : null}
    </ScrollView>
  );
}

function TelemetryError({ error }: { error: Error }) {
  if (!(error instanceof ApiError)) {
    return (
      <Alert variant="danger" title="telemetry failed">
        {error.message}
      </Alert>
    );
  }

  if (error.kind === "unauthorized") {
    return (
      <Alert variant="danger" title="token rejected">
        the device refused the api token — open the Device tab and re-enter it
      </Alert>
    );
  }

  if (error.kind === "transport") {
    return (
      <Alert variant="warning" title="device unreachable">
        no answer from the controller — it may be off, or on another network
      </Alert>
    );
  }

  if (error.kind === "malformed") {
    return (
      <Alert variant="danger" title="unexpected telemetry">
        {error.detail ?? error.message}
      </Alert>
    );
  }

  return (
    <Alert variant="danger" title="device returned an error">
      {error.detail ?? error.message}
    </Alert>
  );
}
