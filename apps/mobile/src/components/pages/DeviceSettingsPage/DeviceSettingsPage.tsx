import { router } from "expo-router";
import { ScrollView } from "react-native";
import { LanguageSwitcher } from "@/components/molecules";
import { DeviceForm } from "@/components/organisms";
import { screenStyles, spacing } from "@aquarium/theme";
import { useDevice } from "@/components/utility/app-providers";

export function DeviceSettingsPage() {
  const { device, setDevice } = useDevice();

  return (
    <ScrollView
      style={screenStyles.page}
      contentContainerStyle={{ padding: spacing.sm, gap: spacing.md }}
    >
      <DeviceForm
        device={device}
        onConnected={async (next) => {
          await setDevice(next);
          router.replace("/");
        }}
      />

      <LanguageSwitcher />
    </ScrollView>
  );
}
