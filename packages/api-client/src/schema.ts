import { z } from "zod";
import type {
  ChannelStatus,
  PumpSettings,
  PumpsStatus,
  TankTelemetryDto,
} from "./generated";
import { ApiError } from "./transport";

const heaterStatusGuard = z.object({
  active: z.boolean(),
  mode: z.enum(["auto", "manual_on", "manual_off"]),
  fault: z
    .enum(["sensor_fault", "over_temperature", "unavailable", "output_error"])
    .nullable(),
  override_expires_in_s: z.number().nullable(),
  target_celsius: z.number(),
});

const pumpsGuard = z.object({
  return: z.boolean(),
  uv: z.boolean(),
  makeup: z.boolean(),
  drain: z.boolean(),
  operation_kind: z.enum(["top_off", "water_change"]).nullable(),
  operation_state: z.enum([
    "idle",
    "draining",
    "settling",
    "refilling",
    "resuming",
    "suspended",
  ]),
  circulation_starved: z.boolean().optional(),
  level_sensors_suspect: z.boolean().optional(),
  return_held_after_drain: z.boolean().optional(),
  invalidation_pending: z.boolean().optional(),
  supervision: z.enum(["active", "unavailable", "off"]).optional(),
});

const waterLevelEntryGuard = z.object({
  wet: z.boolean(),
  faulty: z.boolean(),
});

const telemetryGuard = z.object({
  timestamp_epoch_s: z.number(),
  system_mode: z.enum(["normal", "maintenance", "emergency_lockout"]),
  system_health: z.object({
    uptime_seconds: z.number(),
    wifi_rssi_dbm: z.number().nullable(),
    free_heap_kb: z.number(),
  }),
  temperature: z.object({
    tank_celsius: z.number().nullable(),
    sump_celsius: z.number().nullable(),
    sensors_healthy: z.boolean(),
  }),
  heaters: z.object({
    tank: heaterStatusGuard,
    sump: heaterStatusGuard,
  }),
  pumps: pumpsGuard,
  water_level: z.object({
    sump_mechanical: waterLevelEntryGuard,
    sump_biological: waterLevelEntryGuard,
    sump_pump: waterLevelEntryGuard,
    main_tank: waterLevelEntryGuard,
    sump_reserve: waterLevelEntryGuard,
    sump_return_level: waterLevelEntryGuard,
    reservoir_ready: waterLevelEntryGuard,
    reservoir_full: waterLevelEntryGuard,
    sensors_healthy: z.boolean(),
    available: z.boolean().optional(),
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

const operationKind = z.enum(["top_off", "water_change"]);

const blocker = z.enum([
  "sump_low",
  "reservoir_empty",
  "tank_overflow",
  "levels_unavailable",
  "return_off",
  "sump_full",
  "recent_water_change",
  "reservoir_low",
  "water_change",
  "sump_high",
  "interlock",
  "levels_unavailable_after_drain",
  "supervision_unavailable",
]);

const channelStatusGuard = z.object({
  on: z.boolean(),
  mode: z.enum(["auto", "manual_off"]),
  override_expires_in_s: z.number().nullable(),
  fault: z.enum(["output_error", "unavailable"]).nullable(),
  blocked_by: blocker.nullable(),
  runtime_s: z.number(),
  runtime_reset_at: z.number().nullable(),
});

const operationRecordGuard = z.object({
  kind: operationKind,
  started_at: z.number().nullable(),
  drain_ran_s: z.number(),
  settle_ran_s: z.number(),
  refill_ran_s: z.number(),
  result: z.enum([
    "completed",
    "overflow",
    "sump_full",
    "reservoir_empty",
    "aborted",
    "interrupted_by_reboot",
    "refill_timeout",
    "incomplete",
    "drain_timeout",
  ]),
  resumed: z.boolean(),
  finished_at: z.number().nullable().exactOptional(),
});

const pumpsStatusGuard = z.object({
  channels: z.object({
    return: channelStatusGuard,
    uv: channelStatusGuard,
    makeup: channelStatusGuard,
    drain: channelStatusGuard,
  }),
  operation: z.object({
    kind: operationKind.nullable(),
    state: pumpsGuard.shape.operation_state,
    phase_remaining_s: z.number().nullable(),
    drain_ran_s: z.number(),
    settle_ran_s: z.number(),
    refill_ran_s: z.number(),
    drained_l: z.number().nullable(),
    refilled_l: z.number().nullable(),
  }),
  last_top_off: operationRecordGuard.nullable(),
  last_water_change: operationRecordGuard.nullable(),
  last_skipped: z
    .object({
      kind: operationKind,
      index: z.number(),
      reason: z.enum(["busy", "blocked", "storage"]),
      blocked_by: blocker.nullable(),
      at: z.number().nullable(),
    })
    .nullable(),
  circulation_starved: z.boolean(),
  level_sensors_suspect: z.boolean(),
  return_held_after_drain: z.boolean(),
  invalidation_pending: z.boolean(),
  supervision: pumpsGuard.shape.supervision.unwrap(),
});

const pumpSettingsGuard = z.object({
  top_off: z.array(z.object({ cron: z.string(), duration_s: z.number() })),
  water_change: z.array(
    z.object({
      cron: z.string(),
      drain_s: z.number(),
      settle_s: z.number(),
      refill_s: z.number(),
    }),
  ),
  flow_lph: z.object({
    makeup: z.number().nullable(),
    drain: z.number().nullable(),
  }),
  resume_window_s: z.number(),
  refill_stop_sensor: z
    .enum([
      "sump_mechanical",
      "sump_biological",
      "sump_pump",
      "main_tank",
      "sump_reserve",
      "sump_return_level",
      "reservoir_ready",
      "reservoir_full",
    ])
    .nullable(),
  min_water_change_gap_h: z.number(),
  reservoir_ready_l: z.number().nullable(),
});

const parseWith = <T>(guard: z.ZodType<T>, value: unknown, what: string): T => {
  const result = guard.safeParse(value);

  if (!result.success) {
    const where = result.error.issues
      .map((issue) => issue.path.join("."))
      .filter((path) => path.length > 0)
      .join(", ");

    throw new ApiError(
      "malformed",
      `device ${what} is missing or malformed fields: ${where}`,
      undefined,
      where,
    );
  }

  return result.data;
};

export const assertPumpsStatus = (value: unknown): PumpsStatus =>
  parseWith<PumpsStatus>(pumpsStatusGuard, value, "pump status");

export const assertPumpSettings = (value: unknown): PumpSettings =>
  parseWith<PumpSettings>(pumpSettingsGuard, value, "pump settings");

export const assertChannelStatus = (value: unknown): ChannelStatus =>
  parseWith<ChannelStatus>(channelStatusGuard, value, "pump channel");
