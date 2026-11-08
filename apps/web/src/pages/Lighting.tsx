import { useTelemetry, type Transport } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { ScreenMessage } from "@/components/molecules";
import { LightingControl } from "@/components/organisms/LightingControl/LightingControl";
import { DeviceGate } from "@/components/utility/DeviceGate";
import { errorCodeFor } from "@/pages/Error/error-code";
import { ErrorPage } from "@/pages/Error/Error";

export function Lighting() {
  return (
    <DeviceGate>
      {(transport) => <LightingContent transport={transport} />}
    </DeviceGate>
  );
}

function LightingContent({ transport }: { transport: Transport }) {
  const t = useT();
  const { data, error, isPending } = useTelemetry(transport);

  if (isPending)
    return <ScreenMessage loading>{t("lighting.loading")}</ScreenMessage>;
  if (error && !data) return <ErrorPage code={errorCodeFor(error)} />;
  if (!data) return <ScreenMessage>{t("common.noData")}</ScreenMessage>;

  return <LightingControl transport={transport} lighting={data.lighting} />;
}
