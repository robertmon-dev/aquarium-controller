import type { DeviceConfig } from "./config";

export const queryKeys = {
  telemetry: (device: DeviceConfig) =>
    ["telemetry", device.host, device.port] as const,
  health: (device: DeviceConfig) =>
    ["health", device.host, device.port] as const,
  lightingOverride: (device: DeviceConfig) =>
    ["lighting-override", device.host, device.port] as const,
  lightingSchedule: (device: DeviceConfig, month: number) =>
    ["lighting-schedule", device.host, device.port, month] as const,
};
