import { useTelemetry, type Transport } from "@aquarium/api-client";
import { screenStyles, spacing } from "@aquarium/theme";
import { ScrollView } from "react-native";
import { ScreenMessage } from "@/components/molecules";
import { LightingControl } from "@/components/organisms";
import { useDevice } from "@/components/utility/app-providers";

export function LightingPage() {
  const { transport } = useDevice();

  if (!transport) return <ScreenMessage>no device configured</ScreenMessage>;

  return <LightingContent transport={transport} />;
}

function LightingContent({ transport }: { transport: Transport }) {
  const { data, isPending } = useTelemetry(transport);

  if (isPending)
    return <ScreenMessage loading>reading lighting…</ScreenMessage>;
  if (!data) return <ScreenMessage>no data</ScreenMessage>;

  return (
    <ScrollView
      style={screenStyles.page}
      contentContainerStyle={{ padding: spacing.sm, gap: spacing.sm }}
    >
      <LightingControl transport={transport} lighting={data.lighting} />
    </ScrollView>
  );
}
