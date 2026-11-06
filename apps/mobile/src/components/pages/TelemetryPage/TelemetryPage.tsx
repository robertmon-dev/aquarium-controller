import { Link } from "expo-router";
import { ScrollView, Text, View } from "react-native";
import { Button } from "@/components/atoms";
import { ScreenMessage } from "@/components/molecules";
import { TelemetryPanel } from "@/components/organisms";
import { colors, spacing, typography } from "@/components/utility/theme";
import { useDevice } from "@/components/utility/app-providers";
import { useTelemetryStream } from "@/device/use-telemetry-stream";

export function TelemetryPage() {
  const { transport, loading } = useDevice();
  const streamState = useTelemetryStream(transport);

  if (loading) return <ScreenMessage loading>loading…</ScreenMessage>;

  if (!transport) {
    return (
      <ScreenMessage>
        <View style={{ alignItems: "center", gap: spacing.sm }}>
          <Text
            style={{ color: colors.textMain, fontSize: typography.size.md }}
          >
            no device configured
          </Text>
          <Link href="/settings" asChild>
            <Button variant="primary">open settings</Button>
          </Link>
        </View>
      </ScreenMessage>
    );
  }

  return (
    <ScrollView
      style={{ backgroundColor: colors.bgApp }}
      contentContainerStyle={{ padding: spacing.sm, gap: spacing.sm }}
    >
      <TelemetryPanel transport={transport} streamState={streamState} />
      <Link href="/settings" asChild>
        <Button variant="ghost">settings</Button>
      </Link>
    </ScrollView>
  );
}
