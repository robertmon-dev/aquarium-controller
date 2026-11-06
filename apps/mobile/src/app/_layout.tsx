import { Slot } from "expo-router";
import { AppProviders } from "@/components/utility/app-providers";

export default function RootLayout() {
  return (
    <AppProviders>
      <Slot />
    </AppProviders>
  );
}
