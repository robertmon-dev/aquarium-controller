import { describe, expect, it } from "vitest";
import { assertTelemetry } from "./schema";

const valid = {
  timestamp_epoch_s: 1,
  system_mode: "normal",
  system_health: { uptime_seconds: 5, wifi_rssi_dbm: -50, free_heap_kb: 100 },
  temperature: {
    tank_celsius: 25.5,
    sump_celsius: null,
    target_celsius: 26,
    heater_active: false,
    sensors_healthy: true,
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
