import type { Transport } from "@aquarium/api-client";
import { router } from "expo-router";
import type { ReactNode } from "react";
import { DeviceNotice } from "@/components/molecules/DeviceNotice/DeviceNotice";
import { useDevice } from "@/components/utility/app-providers";
import { useT } from "@/i18n/use-locale";

export function DeviceGate({
  children,
}: {
  children: (transport: Transport) => ReactNode;
}) {
  const t = useT();
  const { transport, loading } = useDevice();

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
