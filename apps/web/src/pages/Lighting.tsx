import { useTelemetry, type Transport } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { ScreenMessage } from "@/components/molecules";
import { LightingControl } from "@/components/organisms/LightingControl/LightingControl";
import { useDevice } from "@/providers/AppProviders";

export function Lighting() {
  const t = useT();
  const { transport, loading } = useDevice();

  if (loading)
    return <ScreenMessage loading>{t("lighting.loading")}</ScreenMessage>;
  if (!transport) return <ScreenMessage>{t("common.noDevice")}</ScreenMessage>;

  return <LightingContent transport={transport} />;
}

function LightingContent({ transport }: { transport: Transport }) {
  const t = useT();
  const { data, isPending } = useTelemetry(transport);

  if (isPending)
    return <ScreenMessage loading>{t("lighting.loading")}</ScreenMessage>;
  if (!data) return <ScreenMessage>{t("common.noData")}</ScreenMessage>;

  return <LightingControl transport={transport} lighting={data.lighting} />;
}
