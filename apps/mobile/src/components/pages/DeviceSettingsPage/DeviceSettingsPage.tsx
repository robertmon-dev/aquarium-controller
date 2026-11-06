import { router } from "expo-router";
import { ScrollView } from "react-native";
import { DeviceForm } from "@/components/organisms";
import { colors, spacing } from "@/components/utility/theme";
import { useDevice } from "@/components/utility/app-providers";

export function DeviceSettingsPage() {
  const { device, setDevice } = useDevice();

  return (
    <ScrollView
      style={{ backgroundColor: colors.bgApp }}
      contentContainerStyle={{ padding: spacing.sm }}
    >
      <DeviceForm
        device={device}
        onConnected={async (next) => {
          await setDevice(next);
          router.replace("/");
        }}
      />
    </ScrollView>
  );
}
