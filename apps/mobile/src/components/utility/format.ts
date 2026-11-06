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

export function formatPercent(value: number | null): string {
  return value === null ? EM_DASH : `${value}%`;
}

export function formatPercentPair(white: number, rgb: number): string {
  return `${white}% / ${rgb}%`;
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

export function formatHeap(kilobytes: number): string {
  return kilobytes < 1000
    ? `${kilobytes} kB`
    : `${(kilobytes / 1024).toFixed(1)} MB`;
}

export function formatRssi(dbm: number): string {
  return `${dbm} dBm`;
}
