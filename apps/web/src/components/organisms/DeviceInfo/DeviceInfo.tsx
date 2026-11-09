import {
  useHealth,
  type DeviceConfig,
  type Transport,
} from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { formatDuration, formatHeap, formatRssi } from "@aquarium/view-model";
import { MetricCard } from "@/components/molecules";
import type { ReactNode } from "react";

export const DeviceInfo = ({
  device,
  transport,
}: {
  device: DeviceConfig;
  transport: Transport;
}): ReactNode => {
  const t = useT();
  const { data, error, isPending } = useHealth(transport);

  const status = error ? "degraded" : isPending ? "off" : "ok";
  const statusLabel = error
    ? t("deviceInfo.offline")
    : isPending
      ? t("deviceInfo.checking")
      : t("deviceInfo.connected");

  const metrics = [
    {
      label: t("deviceInfo.address"),
      value: `${device.host}:${device.port}`,
    },
    ...(data
      ? [
          {
            label: t("cards.system.wifi"),
            value: formatRssi(data.wifi_rssi_dbm),
          },
          {
            label: t("cards.system.freeHeap"),
            value: formatHeap(data.free_heap_kb),
          },
          {
            label: t("cards.system.uptime"),
            value: formatDuration(data.uptime_seconds),
          },
        ]
      : []),
  ];

  return (
    <MetricCard
      title={t("deviceInfo.title")}
      status={status}
      statusLabel={statusLabel}
      metrics={metrics}
    />
  );
};
