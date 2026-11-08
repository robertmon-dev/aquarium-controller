import { useTelemetry, type Transport } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { Alert } from "@/components/atoms";
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

      {data ? <TelemetryDashboard snapshot={data} /> : null}
    </>
  );
}
