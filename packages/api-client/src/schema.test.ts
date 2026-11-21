import { describe, expect, it } from "vitest";
import { assertTelemetry } from "./schema";

const valid = {
  timestamp_epoch_s: 1,
  system_mode: "normal",
  system_health: { uptime_seconds: 5, wifi_rssi_dbm: -50, free_heap_kb: 100 },
  temperature: {
    tank_celsius: 25.5,
    sump_celsius: null,
    sensors_healthy: true,
  },
  heaters: {
    tank: {
      active: true,
      mode: "auto",
      fault: null,
      override_expires_in_s: null,
      target_celsius: 26,
    },
    sump: {
      active: false,
      mode: "manual_off",
      fault: "sensor_fault",
      override_expires_in_s: 120,
      target_celsius: 25,
    },
  },
  pumps: {
    return: true,
    uv: true,
    makeup: false,
    drain: false,
    operation_kind: null,
    operation_state: "idle",
    circulation_starved: false,
    level_sensors_suspect: false,
  },
  lighting: {
    profile: "daylight",
    channel_white_pct: 60,
    channel_rgb_pct: 40,
    override_active: false,
  },
  leak_detected: false,
};

describe("assertTelemetry", () => {
  it("accepts a snapshot carrying the fields the app reads", () => {
    expect(assertTelemetry(valid).timestamp_epoch_s).toBe(1);
  });

  it("accepts an unreadable wifi signal as null", () => {
    const snapshot = {
      ...valid,
      system_health: { ...valid.system_health, wifi_rssi_dbm: null },
    };

    expect(assertTelemetry(snapshot).system_health.wifi_rssi_dbm).toBeNull();
  });

  it("accepts a null tank temperature", () => {
    const snapshot = {
      ...valid,
      temperature: { ...valid.temperature, tank_celsius: null },
    };

    expect(assertTelemetry(snapshot).temperature.tank_celsius).toBeNull();
  });

  it("rejects a snapshot missing the temperature block", () => {
    const { temperature, ...rest } = valid;
    void temperature;

    expect(() => assertTelemetry(rest)).toThrow(/temperature/);
  });

  it("rejects a snapshot missing the heaters block", () => {
    const { heaters, ...rest } = valid;
    void heaters;

    expect(() => assertTelemetry(rest)).toThrow(/heaters/);
  });

  it("rejects a snapshot missing the pumps block", () => {
    const { pumps, ...rest } = valid;
    void pumps;

    expect(() => assertTelemetry(rest)).toThrow(/pumps/);
  });

  it("rejects an unknown operation state", () => {
    expect(() =>
      assertTelemetry({
        ...valid,
        pumps: { ...valid.pumps, operation_state: "flooding" },
      }),
    ).toThrow(/pumps\.operation_state/);
  });

  it("accepts pumps from firmware without the starved or suspect flags", () => {
    const { circulation_starved, level_sensors_suspect, ...pumps } =
      valid.pumps;
    void circulation_starved;
    void level_sensors_suspect;

    expect(assertTelemetry({ ...valid, pumps }).pumps.return).toBe(true);
  });

  it("rejects a non-boolean suspect harness flag", () => {
    expect(() =>
      assertTelemetry({
        ...valid,
        pumps: { ...valid.pumps, level_sensors_suspect: "yes" },
      }),
    ).toThrow(/pumps\.level_sensors_suspect/);
  });

  it("rejects an unknown heater mode", () => {
    const snapshot = {
      ...valid,
      heaters: {
        ...valid.heaters,
        tank: { ...valid.heaters.tank, mode: "turbo" },
      },
    };

    expect(() => assertTelemetry(snapshot)).toThrow(/heaters\.tank\.mode/);
  });

  it("accepts a heater whose output could not be driven", () => {
    const snapshot = {
      ...valid,
      heaters: {
        ...valid.heaters,
        tank: { ...valid.heaters.tank, active: true, fault: "output_error" },
      },
    };

    expect(assertTelemetry(snapshot)).toBe(snapshot);
  });

  it("rejects an unknown system mode", () => {
    expect(() => assertTelemetry({ ...valid, system_mode: "on_fire" })).toThrow(
      /system_mode/,
    );
  });

  it("ignores extra fields a newer firmware may add", () => {
    expect(() =>
      assertTelemetry({ ...valid, future_sensor: { ok: true } }),
    ).not.toThrow();
  });
});
