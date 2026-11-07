import { useTelemetry, type Transport } from "@aquarium/api-client";
import { screenStyles, spacing } from "@aquarium/theme";
import { ScrollView } from "react-native";
import { ScreenMessage } from "@/components/molecules";
import { LightingControl } from "@/components/organisms";
import { useDevice } from "@/components/utility/app-providers";
import { useT } from "@/i18n/use-locale";

export function LightingPage() {
  const t = useT();
  const { transport } = useDevice();

  if (!transport) return <ScreenMessage>{t("common.noDevice")}</ScreenMessage>;

  return <LightingContent transport={transport} />;
}

function LightingContent({ transport }: { transport: Transport }) {
  const t = useT();
  const { data, isPending } = useTelemetry(transport);

  if (isPending)
    return <ScreenMessage loading>{t("lighting.loading")}</ScreenMessage>;
  if (!data) return <ScreenMessage>{t("common.noData")}</ScreenMessage>;

  return (
    <ScrollView
      style={screenStyles.page}
      contentContainerStyle={{ padding: spacing.sm, gap: spacing.sm }}
    >
      <LightingControl transport={transport} lighting={data.lighting} />
    </ScrollView>
  );
}
