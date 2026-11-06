import type { TankTelemetryDto } from "@aquarium/api-client";
import type { Metric } from "@/components/molecules";
import { formatCelsius, formatPercentPair } from "@/components/utility/format";

export function telemetryMetrics(snapshot: TankTelemetryDto): Metric[] {
  return [
    { label: "mode", value: snapshot.system_mode },
    { label: "tank", value: formatCelsius(snapshot.temperature.tank_celsius) },
    { label: "sump", value: formatCelsius(snapshot.temperature.sump_celsius) },
    {
      label: "target",
      value: formatCelsius(snapshot.temperature.target_celsius),
    },
    {
      label: "heater",
      value: snapshot.temperature.heater_active ? "on" : "off",
    },
    { label: "lighting", value: snapshot.lighting.profile },
    {
      label: "white / rgb",
      value: formatPercentPair(
        snapshot.lighting.channel_white_pct,
        snapshot.lighting.channel_rgb_pct,
      ),
    },
    {
      label: "override",
      value: snapshot.lighting.override_active ? "active" : "none",
    },
    { label: "leak", value: snapshot.leak_detected ? "DETECTED" : "none" },
    { label: "uptime", value: `${snapshot.system_health.uptime_seconds}s` },
    { label: "wifi", value: `${snapshot.system_health.wifi_rssi_dbm} dBm` },
    { label: "free heap", value: `${snapshot.system_health.free_heap_kb} kB` },
  ];
}
