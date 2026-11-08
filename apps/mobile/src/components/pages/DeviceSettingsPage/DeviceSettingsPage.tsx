import { router } from "expo-router";
import { ScrollView } from "react-native";
import { LanguageSwitcher } from "@/components/molecules";
import { DeviceForm, DeviceInfo } from "@/components/organisms";
import { screenStyles, spacing } from "@aquarium/theme";
import { useDevice } from "@/components/utility/app-providers";

export function DeviceSettingsPage() {
  const { device, transport, setDevice } = useDevice();

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

      {device && transport ? (
        <DeviceInfo device={device} transport={transport} />
      ) : null}

      <LanguageSwitcher />
    </ScrollView>
  );
}
