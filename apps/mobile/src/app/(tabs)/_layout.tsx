import { colors, screenStyles, typography } from "@aquarium/theme";
import Ionicons from "@expo/vector-icons/Ionicons";
import { Tabs } from "expo-router";
import type { ColorValue } from "react-native";
import { Badge } from "@/components/atoms";
import { ScreenMessage } from "@/components/molecules";
import { useDevice } from "@/components/utility/app-providers";
import { streamBadge } from "@/components/utility/stream-badge";
import { useTelemetryStream } from "@/device/use-telemetry-stream";

function iconColor(color: ColorValue | undefined): string {
  return typeof color === "string" ? color : colors.textMuted;
}

export default function TabsLayout() {
  const { transport, loading } = useDevice();
  const streamState = useTelemetryStream(transport);
  const badge = streamBadge(streamState);

  if (loading) return <ScreenMessage loading>loading…</ScreenMessage>;

  return (
    <Tabs
      screenOptions={{
        headerStyle: screenStyles.navigation.header,
        headerTintColor: colors.textMain,
        headerTitleStyle: { fontWeight: typography.weight.bold },
        headerRight: () => (
          <Badge variant={badge.variant} style={{ marginRight: 12 }}>
            {badge.label}
          </Badge>
        ),
        sceneStyle: screenStyles.page,
        tabBarStyle: screenStyles.navigation.tabBar,
        tabBarActiveTintColor: colors.primary,
        tabBarInactiveTintColor: colors.textMuted,
      }}
    >
      <Tabs.Screen
        name="index"
        options={{
          title: "Telemetry",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="pulse" color={iconColor(color)} size={size} />
          ),
        }}
      />
      <Tabs.Screen
        name="lighting"
        options={{
          title: "Lighting",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="sunny" color={iconColor(color)} size={size} />
          ),
        }}
      />
      <Tabs.Screen
        name="settings"
        options={{
          title: "Device",
          tabBarIcon: ({ color, size }) => (
            <Ionicons
              name="hardware-chip"
              color={iconColor(color)}
              size={size}
            />
          ),
        }}
      />
    </Tabs>
  );
}
