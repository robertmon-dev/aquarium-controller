import type { Transport } from "@aquarium/api-client";
import { router } from "expo-router";
import type { ReactNode } from "react";
import { DeviceNotice } from "@/components/molecules/DeviceNotice/DeviceNotice";
import { useDevice } from "@/components/utility/app-providers";
import { useT } from "@/i18n/use-locale";

/**
 * Pages that read the device get a non-null transport or an explanation with a
 * way forward. Previously they announced that they were reading telemetry while
 * really waiting on SecureStore, then said "no device" in the same flat style
 * as a measured value.
 */
export function DeviceGate({
  children,
}: {
  children: (transport: Transport) => ReactNode;
}) {
  const t = useT();
  const { transport, loading } = useDevice();

  // Only the saved config coming back. A message would flash and say nothing.
  if (loading) return null;

  if (!transport)
    return (
      <DeviceNotice
        icon="hardware-chip-outline"
        title={t("errors.notConfigured.title")}
        message={t("errors.notConfigured.description")}
        actionLabel={t("errors.openSettings")}
        onAction={() => router.replace("/settings")}
      />
    );

  return <>{children(transport)}</>;
}
