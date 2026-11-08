import type { TankTelemetryDto } from "@aquarium/api-client";
import type { Translate } from "@aquarium/i18n";
import type { Metric, StatusTone } from "./semantics";
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

function translateProfile(profile: string, t: Translate): string {
  const known = ["off", "sunrise", "daylight", "sunset", "moonlight"];
  return known.includes(profile)
    ? t(`profiles.${profile}` as Parameters<Translate>[0])
    : profile;
}

export type HeroCardData = CardData & {
  heroValue: string;
  heroUnit: string;
};

const onOff = (t: Translate, on: boolean) =>
  on ? t("common.on") : t("common.off");

export function temperatureCard(
  s: TankTelemetryDto,
  t: Translate,
): HeroCardData {
  const {
    tank_celsius,
    sump_celsius,
    target_celsius,
    heater_active,
    sensors_healthy,
  } = s.temperature;

  return {
    title: t("cards.temperature.title"),
    status: sensors_healthy ? "ok" : "off",
    statusLabel: sensors_healthy ? t("common.healthy") : t("common.sensorsOff"),
    heroValue: tank_celsius === null ? EM_DASH : tank_celsius.toFixed(2),
    heroUnit: "°C",
    metrics: [
      {
        label: t("cards.temperature.target"),
        value: formatCelsius(target_celsius),
      },
      { label: t("cards.temperature.heater"), value: onOff(t, heater_active) },
      {
        label: t("cards.temperature.sump"),
        value: formatCelsius(sump_celsius),
      },
    ],
  };
}

export function filtrationCard(s: TankTelemetryDto, t: Translate): CardData {
  const { flow_rate_lph, flow_ok, filter_relay_active } = s.filtration;

  const status: StatusTone =
    flow_rate_lph === null ? "off" : flow_ok ? "ok" : "degraded";

  return {
    title: t("cards.filtration.title"),
    status,
    statusLabel:
      flow_rate_lph === null
        ? t("common.off")
        : flow_ok
          ? t("cards.filtration.flowing")
          : t("cards.filtration.noFlow"),
    metrics: [
      {
        label: t("cards.filtration.flow"),
        value: formatLitresPerHour(flow_rate_lph),
      },
      {
        label: t("cards.filtration.relay"),
        value: onOff(t, filter_relay_active),
      },
    ],
  };
}

export function co2Card(s: TankTelemetryDto, t: Translate): CardData {
  const {
    solenoid_open,
    current_ph,
    drop_checker_ph_target,
    ph_electrode_health,
  } = s.co2;

  return {
    title: t("cards.co2.title"),
    status: current_ph === null ? "off" : "ok",
    statusLabel:
      current_ph === null
        ? t("common.off")
        : solenoid_open
          ? t("cards.co2.dosing")
          : t("common.idle"),
    metrics: [
      { label: t("cards.co2.ph"), value: formatPh(current_ph) },
      {
        label: t("cards.co2.target"),
        value: formatPh(drop_checker_ph_target),
      },
      { label: t("cards.co2.solenoid"), value: onOff(t, solenoid_open) },
      {
        label: t("cards.co2.electrode"),
        value:
          ph_electrode_health === null
            ? EM_DASH
            : formatPercent(Math.round(ph_electrode_health * 100)),
      },
    ],
  };
}

export function atoCard(s: TankTelemetryDto, t: Translate): CardData {
  const {
    sump_water_low,
    reservoir_empty,
    pump_active,
    last_fill_duration_ms,
  } = s.ato;

  const statusLabel = reservoir_empty
    ? t("cards.ato.reservoirEmpty")
    : sump_water_low
      ? t("cards.ato.sumpLow")
      : pump_active
        ? t("cards.ato.filling")
        : t("common.idle");

  return {
    title: t("cards.ato.title"),
    status: reservoir_empty || sump_water_low ? "degraded" : "ok",
    statusLabel,
    metrics: [
      { label: t("cards.ato.pump"), value: onOff(t, pump_active) },
      {
        label: t("cards.ato.sumpLow"),
        value: sump_water_low ? t("common.yes") : t("common.no"),
      },
      {
        label: t("cards.ato.reservoir"),
        value: reservoir_empty ? t("cards.ato.empty") : t("common.ok"),
      },
      {
        label: t("cards.ato.lastFill"),
        value: formatDuration(Math.round(last_fill_duration_ms / 1000)),
      },
    ],
  };
}

export function lightingCard(s: TankTelemetryDto, t: Translate): CardData {
  const { profile, channel_white_pct, channel_rgb_pct, override_active } =
    s.lighting;

  return {
    title: t("cards.lighting.title"),
    status: override_active ? "degraded" : "ok",
    statusLabel: override_active
      ? t("cards.lighting.override")
      : translateProfile(profile, t),
    metrics: [
      {
        label: t("cards.lighting.profile"),
        value: translateProfile(profile, t),
      },
      {
        label: t("cards.lighting.whiteRgb"),
        value: formatPercentPair(channel_white_pct, channel_rgb_pct),
      },
    ],
  };
}

export function systemCard(s: TankTelemetryDto, t: Translate): CardData {
  const { uptime_seconds, wifi_rssi_dbm, free_heap_kb } = s.system_health;

  return {
    title: t("cards.system.title"),
    status: s.system_mode === "normal" ? "ok" : "degraded",
    statusLabel:
      s.system_mode === "normal" ? t("systemModes.normal") : s.system_mode,
    metrics: [
      { label: t("cards.system.wifi"), value: formatRssi(wifi_rssi_dbm) },
      { label: t("cards.system.freeHeap"), value: formatHeap(free_heap_kb) },
      {
        label: t("cards.system.uptime"),
        value: formatDuration(uptime_seconds),
      },
    ],
  };
}
