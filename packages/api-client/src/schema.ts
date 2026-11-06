import { z } from "zod";
import type { TankTelemetryDto } from "./generated";
import { ApiError } from "./transport";

const telemetryGuard = z.object({
  timestamp_epoch_s: z.number(),
  system_mode: z.enum(["normal", "maintenance", "emergency_lockout"]),
  system_health: z.object({
    uptime_seconds: z.number(),
    wifi_rssi_dbm: z.number(),
    free_heap_kb: z.number(),
  }),
  temperature: z.object({
    tank_celsius: z.number().nullable(),
    sump_celsius: z.number().nullable(),
    target_celsius: z.number(),
    heater_active: z.boolean(),
    sensors_healthy: z.boolean(),
  }),
  lighting: z.object({
    profile: z.enum(["off", "sunrise", "daylight", "sunset", "moonlight"]),
    channel_white_pct: z.number(),
    channel_rgb_pct: z.number(),
    override_active: z.boolean(),
  }),
  leak_detected: z.boolean(),
});

export function assertTelemetry(value: unknown): TankTelemetryDto {
  const result = telemetryGuard.safeParse(value);

  if (!result.success) {
    const where = result.error.issues
      .map((issue) => issue.path.join("."))
      .filter((path) => path.length > 0)
      .join(", ");

    throw new ApiError(
      "malformed",
      `device telemetry is missing or malformed fields: ${where}`,
      undefined,
      where,
    );
  }

  return value as TankTelemetryDto;
}
