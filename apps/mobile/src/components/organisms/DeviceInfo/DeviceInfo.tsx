import {
  useHealth,
  type DeviceConfig,
  type Transport,
} from "@aquarium/api-client";
import { formatDuration, formatHeap, formatRssi } from "@aquarium/view-model";
import { MetricCard } from "@/components/molecules";
import { useT } from "@/i18n/use-locale";

export function DeviceInfo({
  device,
  transport,
}: {
  device: DeviceConfig;
  transport: Transport;
}) {
  const t = useT();
  const { data, error, isPending } = useHealth(transport);

  // No firmware row: SystemHealthDto reports uptime, RSSI and free heap, and
  // nothing in the API reports a build.
  return (
    <MetricCard
      title={t("deviceInfo.title")}
      status={error ? "degraded" : isPending ? "off" : "ok"}
      statusLabel={
        error
          ? t("deviceInfo.offline")
          : isPending
            ? t("deviceInfo.checking")
            : t("deviceInfo.connected")
      }
      metrics={[
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
      ]}
    />
  );
}
