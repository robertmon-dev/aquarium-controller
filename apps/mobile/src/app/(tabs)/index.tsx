import { Redirect } from "expo-router";
import { TelemetryPage } from "@/components/pages";
import { useDevice } from "@/components/utility/app-providers";

export default function Index() {
  const { device } = useDevice();

  if (!device) return <Redirect href="/settings" />;

  return <TelemetryPage />;
}
