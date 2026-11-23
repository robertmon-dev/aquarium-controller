import { useResetFlood, useTelemetry, type Transport } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { Alert, Button } from "@/components/atoms";
import { canResetFlood } from "@aquarium/view-model";
import { ScreenMessage } from "@/components/molecules";
import { TelemetryDashboard } from "@/components/organisms/TelemetryDashboard/TelemetryDashboard";
import { DeviceGate } from "@/components/utility/DeviceGate";
import { errorCodeFor } from "@/pages/Error/error-code";
import { ErrorPage } from "@/pages/Error/Error";

export function Dashboard() {
  return (
    <DeviceGate>
      {(transport) => <DashboardContent transport={transport} />}
    </DeviceGate>
  );
}

function DashboardContent({ transport }: { transport: Transport }) {
  const t = useT();
  const { data, error, isPending } = useTelemetry(transport);
  const resetFlood = useResetFlood(transport);

  if (isPending)
    return <ScreenMessage loading>{t("telemetry.loading")}</ScreenMessage>;

  if (error && !data) return <ErrorPage code={errorCodeFor(error)} />;

  return (
    <>
      {error ? (
        <Alert variant="warning" title={t("telemetry.failedTitle")}>
          {error.message}
        </Alert>
      ) : null}

      {data && canResetFlood(data) ? (
        <>
          <Button onClick={() => resetFlood.mutate()} isLoading={resetFlood.isPending}>
            {t("telemetry.resetFlood")}
          </Button>
          {resetFlood.error ? (
            <Alert variant="danger" title={t("telemetry.resetFloodError")}>
              {resetFlood.error.message}
            </Alert>
          ) : null}
        </>
      ) : null}
      {data ? <TelemetryDashboard snapshot={data} /> : null}
    </>
  );
}
