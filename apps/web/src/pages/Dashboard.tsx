import { ApiError, useTelemetry, type Transport } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { Alert } from "@/components/atoms";
import { ScreenMessage } from "@/components/molecules";
import { TelemetryDashboard } from "@/components/organisms/TelemetryDashboard/TelemetryDashboard";
import { useDevice } from "@/providers/AppProviders";

export function Dashboard() {
  const t = useT();
  const { transport, loading } = useDevice();

  if (loading)
    return <ScreenMessage loading>{t("telemetry.loading")}</ScreenMessage>;
  if (!transport) return <ScreenMessage>{t("common.noDevice")}</ScreenMessage>;

  return <DashboardContent transport={transport} />;
}

function DashboardContent({ transport }: { transport: Transport }) {
  const t = useT();
  const { data, error, isPending } = useTelemetry(transport);

  if (isPending)
    return <ScreenMessage loading>{t("telemetry.loading")}</ScreenMessage>;

  return (
    <>
      {error ? <TelemetryError error={error} /> : null}
      {data ? <TelemetryDashboard snapshot={data} /> : null}
    </>
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
