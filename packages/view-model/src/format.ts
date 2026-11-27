import type { ChannelLevels } from "@aquarium/api-client";

export const EM_DASH = "—";

export function formatCelsius(value: number | null): string {
  return value === null ? EM_DASH : `${value.toFixed(2)} °C`;
}

export function formatPh(value: number | null): string {
  return value === null ? EM_DASH : value.toFixed(2);
}

export function formatLitresPerHour(value: number | null): string {
  return value === null ? EM_DASH : `${Math.round(value)} L/h`;
}

export function formatPpm(value: number | null): string {
  return value === null ? EM_DASH : `${Math.round(value)} ppm`;
}

export function formatMicrosiemens(value: number | null): string {
  return value === null ? EM_DASH : `${Math.round(value)} \u00b5S/cm`;
}

export function formatPercent(value: number | null): string {
  return value === null ? EM_DASH : `${value}%`;
}

const CHANNEL_SHORT: Record<keyof ChannelLevels, string> = {
  red_pct: "R",
  green_pct: "G",
  blue_pct: "B",
  white_pct: "W",
};

export function formatChannelLevels(levels: ChannelLevels): string {
  const parts = (Object.keys(CHANNEL_SHORT) as (keyof ChannelLevels)[]).map(
    (key) => `${CHANNEL_SHORT[key]} ${levels[key]}`,
  );
  return `${parts.join(" · ")} %`;
}

export function formatDuration(totalSeconds: number): string {
  if (totalSeconds < 60) return `${totalSeconds}s`;

  if (totalSeconds < 3600) {
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
  }

  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  return `${hours}h ${String(minutes).padStart(2, "0")}m`;
}

export function formatHeap(kibibytes: number): string {
  return kibibytes < 1024
    ? `${kibibytes} KiB`
    : `${(kibibytes / 1024).toFixed(1)} MiB`;
}

export function formatRssi(dbm: number | null): string {
  return dbm === null ? EM_DASH : `${dbm} dBm`;
}
