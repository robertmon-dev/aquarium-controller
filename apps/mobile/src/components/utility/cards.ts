import type { TankTelemetryDto } from "@aquarium/api-client";
import type { StatusTone } from "@aquarium/theme";
import type { Metric } from "@/components/molecules";
import {
  EM_DASH,
  formatCelsius,
  formatDuration,
  formatHeap,
  formatLitresPerHour,
  formatPercent,
  formatPercentPair,
  formatPh,
  formatRssi,
} from "./format";

export interface CardData {
  title: string;
  status: StatusTone;
  statusLabel: string;
  metrics: Metric[];
}

export type HeroCardData = CardData & {
  heroValue: string;
  heroUnit: string;
};

const onOff = (on: boolean) => (on ? "on" : "off");

export function temperatureCard(s: TankTelemetryDto): HeroCardData {
  const {
    tank_celsius,
    sump_celsius,
    target_celsius,
    heater_active,
    sensors_healthy,
  } = s.temperature;

  return {
    title: "temperature",
    status: sensors_healthy ? "ok" : "off",
    statusLabel: sensors_healthy ? "healthy" : "sensors off",
    heroValue: tank_celsius === null ? EM_DASH : tank_celsius.toFixed(2),
    heroUnit: "°C",
    metrics: [
      { label: "target", value: formatCelsius(target_celsius) },
      { label: "heater", value: onOff(heater_active) },
      { label: "sump", value: formatCelsius(sump_celsius) },
    ],
  };
}

export function filtrationCard(s: TankTelemetryDto): CardData {
  const { flow_rate_lph, flow_ok, filter_relay_active } = s.filtration;

  const status: StatusTone =
    flow_rate_lph === null ? "off" : flow_ok ? "ok" : "degraded";

  return {
    title: "filtration",
    status,
    statusLabel:
      flow_rate_lph === null ? "off" : flow_ok ? "flowing" : "no flow",
    metrics: [
      { label: "flow", value: formatLitresPerHour(flow_rate_lph) },
      { label: "relay", value: onOff(filter_relay_active) },
    ],
  };
}

export function co2Card(s: TankTelemetryDto): CardData {
  const {
    solenoid_open,
    current_ph,
    drop_checker_ph_target,
    ph_electrode_health,
  } = s.co2;

  return {
    title: "co2",
    status: current_ph === null ? "off" : "ok",
    statusLabel:
      current_ph === null ? "off" : solenoid_open ? "dosing" : "idle",
    metrics: [
      { label: "pH", value: formatPh(current_ph) },
      { label: "target", value: formatPh(drop_checker_ph_target) },
      { label: "solenoid", value: onOff(solenoid_open) },
      {
        label: "electrode",
        value:
          ph_electrode_health === null
            ? EM_DASH
            : formatPercent(Math.round(ph_electrode_health * 100)),
      },
    ],
  };
}

export function atoCard(s: TankTelemetryDto): CardData {
  const {
    sump_water_low,
    reservoir_empty,
    pump_active,
    last_fill_duration_ms,
  } = s.ato;

  const statusLabel = reservoir_empty
    ? "reservoir empty"
    : sump_water_low
      ? "sump low"
      : pump_active
        ? "filling"
        : "idle";

  return {
    title: "ato",
    status: reservoir_empty || sump_water_low ? "degraded" : "ok",
    statusLabel,
    metrics: [
      { label: "pump", value: onOff(pump_active) },
      { label: "sump low", value: sump_water_low ? "yes" : "no" },
      { label: "reservoir", value: reservoir_empty ? "empty" : "ok" },
      {
        label: "last fill",
        value: formatDuration(Math.round(last_fill_duration_ms / 1000)),
      },
    ],
  };
}

export function lightingCard(s: TankTelemetryDto): CardData {
  const { profile, channel_white_pct, channel_rgb_pct, override_active } =
    s.lighting;

  return {
    title: "lighting",
    status: override_active ? "degraded" : "ok",
    statusLabel: override_active ? "override" : profile,
    metrics: [
      { label: "profile", value: profile },
      {
        label: "white / rgb",
        value: formatPercentPair(channel_white_pct, channel_rgb_pct),
      },
    ],
  };
}

export function systemCard(s: TankTelemetryDto): CardData {
  const { uptime_seconds, wifi_rssi_dbm, free_heap_kb } = s.system_health;

  return {
    title: "system",
    status: s.system_mode === "normal" ? "ok" : "degraded",
    statusLabel: s.system_mode,
    metrics: [
      { label: "wifi", value: formatRssi(wifi_rssi_dbm) },
      { label: "free heap", value: formatHeap(free_heap_kb) },
      { label: "uptime", value: formatDuration(uptime_seconds) },
    ],
  };
}
