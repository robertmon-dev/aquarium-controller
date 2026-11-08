import { ApiError, useTelemetry, type Transport } from "@aquarium/api-client";
import { colors, screenStyles, spacing } from "@aquarium/theme";
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { RefreshControl, ScrollView } from "react-native";
import { Alert } from "@/components/atoms";
import { ScreenMessage } from "@/components/molecules";
import { TelemetryDashboard } from "@/components/organisms";
import { DeviceGate } from "@/components/utility/DeviceGate";
import { useT } from "@/i18n/use-locale";

export function TelemetryPage() {
  return (
    <DeviceGate>
      {(transport) => <TelemetryContent transport={transport} />}
    </DeviceGate>
  );
}

function TelemetryContent({ transport }: { transport: Transport }) {
  const t = useT();
  const queryClient = useQueryClient();
  const [refreshing, setRefreshing] = useState(false);
  const { data, error, isPending } = useTelemetry(transport);

  const onRefresh = useCallback(async () => {
    setRefreshing(true);
    await queryClient.invalidateQueries();
    setRefreshing(false);
  }, [queryClient]);

  if (isPending)
    return <ScreenMessage loading>{t("telemetry.loading")}</ScreenMessage>;

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
  const t = useT();

  if (!(error instanceof ApiError)) {
    return (
      <Alert variant="danger" title={t("telemetry.failedTitle")}>
        {error.message}
      </Alert>
    );
  }

  if (error.kind === "unauthorized") {
    return (
      <Alert variant="danger" title={t("telemetry.unauthorizedTitle")}>
        {t("telemetry.unauthorizedBody")}
      </Alert>
    );
  }

  if (error.kind === "transport") {
    return (
      <Alert variant="warning" title={t("telemetry.unreachableTitle")}>
        {t("telemetry.unreachableBody")}
      </Alert>
    );
  }

  if (error.kind === "malformed") {
    return (
      <Alert variant="danger" title={t("telemetry.malformedTitle")}>
        {error.detail ?? error.message}
      </Alert>
    );
  }

  return (
    <Alert variant="danger" title={t("telemetry.errorTitle")}>
      {error.detail ?? error.message}
    </Alert>
  );
}
