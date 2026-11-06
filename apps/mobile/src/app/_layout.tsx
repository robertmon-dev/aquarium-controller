import { colors, screenStyles } from "@aquarium/theme";
import { Stack } from "expo-router";
import { AppProviders } from "@/components/utility/app-providers";

export default function RootLayout() {
  return (
    <AppProviders>
      <Stack
        screenOptions={{
          headerStyle: screenStyles.navigation.header,
          headerTintColor: colors.textMain,
          headerTitleStyle: screenStyles.navigation.headerTitle,
          contentStyle: screenStyles.page,
        }}
      >
        <Stack.Screen name="index" options={{ title: "Telemetry" }} />
        <Stack.Screen name="settings" options={{ title: "Device" }} />
      </Stack>
    </AppProviders>
  );
}
