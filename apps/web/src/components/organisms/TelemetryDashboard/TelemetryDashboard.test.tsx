import type { TankTelemetryDto } from "@aquarium/api-client";
import { memoryAdapter } from "@aquarium/i18n/adapter";
import { LocaleProvider } from "@aquarium/i18n/use-locale";
import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TelemetryDashboard } from "./TelemetryDashboard";

function snapshot(overrides: Partial<TankTelemetryDto> = {}): TankTelemetryDto {
  return {
    timestamp_epoch_s: 1_750_000_000,
    system_mode: "normal",
    system_health: {
      uptime_seconds: 3600,
      wifi_rssi_dbm: -57,
      free_heap_kb: 180,
    },
    temperature: {
      tank_celsius: 26.5,
      sump_celsius: 26,
      target_celsius: 27,
      heater_active: false,
      sensors_healthy: true,
    },
    filtration: { flow_rate_lph: 420, flow_ok: true, filter_relay_active: true },
    co2: {
      solenoid_open: false,
      current_ph: 6.85,
      drop_checker_ph_target: 6.8,
      ph_electrode_health: 0.94,
    },
    ato: {
      sump_water_low: false,
      reservoir_empty: false,
      pump_active: false,
      last_fill_duration_ms: 4200,
    },
    lighting: {
      profile: "daylight",
      channel_white_pct: 60,
      channel_rgb_pct: 40,
      override_active: false,
    },
    water_level: {
      sump_mechanical: { wet: true, faulty: false },
      sump_biological: { wet: true, faulty: false },
      sump_pump: { wet: true, faulty: false },
      main_tank: { wet: true, faulty: false },
      sump_reserve: { wet: false, faulty: false },
      sensors_healthy: true,
    },
    leak_detected: false,
    ...overrides,
  };
}

function renderDashboard(dto: TankTelemetryDto) {
  return render(
    <LocaleProvider adapter={memoryAdapter({ stored: "en", system: "en" })}>
      <TelemetryDashboard snapshot={dto} />
    </LocaleProvider>,
  );
}

describe("TelemetryDashboard", () => {
  it("shows the tank temperature as the hero value", async () => {
    renderDashboard(snapshot());

    await waitFor(() => expect(screen.getByText("26.50")).toBeTruthy());
    expect(screen.getByText("°C")).toBeTruthy();
  });

  it("raises an alert when a leak is detected", async () => {
    renderDashboard(snapshot({ leak_detected: true }));

    await waitFor(() =>
      expect(screen.getAllByRole("alert").length).toBeGreaterThan(0),
    );
  });

  it("stays quiet when there is no leak", async () => {
    renderDashboard(snapshot());

    await waitFor(() => expect(screen.getByText("26.50")).toBeTruthy());
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
