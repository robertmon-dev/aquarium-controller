import { colors, screenStyles, typography } from "@aquarium/theme";
import Ionicons from "@expo/vector-icons/Ionicons";
import { Tabs } from "expo-router";
import type { ColorValue } from "react-native";
import { Badge } from "@/components/atoms";
import { useDevice } from "@/components/utility/app-providers";
import { streamBadge, useTelemetryStream } from "@aquarium/view-model";
import { useT } from "@/i18n/use-locale";

function iconColor(color: ColorValue | undefined): string {
  return typeof color === "string" ? color : colors.textMuted;
}

export default function TabsLayout() {
  const t = useT();
  const { transport } = useDevice();
  const stream = useTelemetryStream(transport);
  const badge = streamBadge(stream.status, t);

  return (
    <Tabs
      screenOptions={{
        headerStyle: screenStyles.navigation.header,
        headerTintColor: colors.textMain,
        headerTitleStyle: { fontWeight: typography.weight.bold },
        headerRight: () => (
          <Badge
            variant={badge.variant}
            style={{ marginRight: 12, alignSelf: "center" }}
          >
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
          title: t("tabs.telemetry"),
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="pulse" color={iconColor(color)} size={size} />
          ),
        }}
      />
      <Tabs.Screen
        name="lighting"
        options={{
          title: t("tabs.lighting"),
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="sunny" color={iconColor(color)} size={size} />
          ),
        }}
      />
      <Tabs.Screen
        name="settings"
        options={{
          title: t("tabs.device"),
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
