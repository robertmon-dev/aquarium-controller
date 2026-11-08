import type { TankTelemetryDto } from "@aquarium/api-client";
import { describe, expect, it } from "vitest";
import {
  atoCard,
  co2Card,
  filtrationCard,
  lightingCard,
  systemCard,
  temperatureCard,
} from "./cards";
import { translator } from "@aquarium/i18n";

const t = translator("en");

const snapshot: TankTelemetryDto = {
  timestamp_epoch_s: 1,
  system_mode: "normal",
  system_health: {
    uptime_seconds: 3700,
    wifi_rssi_dbm: -54,
    free_heap_kb: 8302,
  },
  temperature: {
    tank_celsius: 25.5,
    sump_celsius: 25.1,
    target_celsius: 27,
    heater_active: true,
    sensors_healthy: true,
  },
  filtration: { flow_rate_lph: 640, flow_ok: true, filter_relay_active: true },
  co2: {
    solenoid_open: true,
    current_ph: 6.8,
    drop_checker_ph_target: 6.8,
    ph_electrode_health: 0.93,
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
};

describe("temperatureCard", () => {
  it("puts the tank reading in the hero slot", () => {
    const card = temperatureCard(snapshot, t);

    expect(card.heroValue).toBe("25.50");
    expect(card.heroUnit).toBe("°C");
    expect(card.status).toBe("ok");
  });

  it("reads as off when the sensors are disabled, not as zero", () => {
    const card = temperatureCard(
      {
        ...snapshot,
        temperature: {
          ...snapshot.temperature,
          tank_celsius: null,
          sump_celsius: null,
          sensors_healthy: false,
        },
      },
      t,
    );

    expect(card.heroValue).toBe("—");
    expect(card.status).toBe("off");
    expect(card.statusLabel).toBe("sensors off");
  });

  it("lists target, heater and sump as metrics", () => {
    const labels = temperatureCard(snapshot, t).metrics.map((m) => m.label);

    expect(labels).toEqual(["target", "heater", "sump"]);
  });
});

describe("filtrationCard", () => {
  it("is ok when flow is ok", () => {
    expect(filtrationCard(snapshot, t).status).toBe("ok");
  });

  it("is degraded when flow is not ok but the pump relay is on", () => {
    const card = filtrationCard(
      {
        ...snapshot,
        filtration: {
          flow_rate_lph: 0,
          flow_ok: false,
          filter_relay_active: true,
        },
      },
      t,
    );

    expect(card.status).toBe("degraded");
    expect(card.statusLabel).toBe("no flow");
  });

  it("is off when there is no reading at all", () => {
    const card = filtrationCard(
      {
        ...snapshot,
        filtration: {
          flow_rate_lph: null,
          flow_ok: false,
          filter_relay_active: false,
        },
      },
      t,
    );

    expect(card.status).toBe("off");
  });
});

describe("co2Card", () => {
  it("shows pH against the drop checker target", () => {
    const card = co2Card(snapshot, t);

    expect(card.metrics.find((m) => m.label === "pH")?.value).toBe("6.80");
    expect(card.metrics.find((m) => m.label === "target")?.value).toBe("6.80");
  });

  it("is off with no pH reading", () => {
    const card = co2Card(
      {
        ...snapshot,
        co2: { ...snapshot.co2, current_ph: null, ph_electrode_health: null },
      },
      t,
    );

    expect(card.status).toBe("off");
  });
});

describe("atoCard", () => {
  it("is degraded when the reservoir is empty", () => {
    const card = atoCard(
      {
        ...snapshot,
        ato: { ...snapshot.ato, reservoir_empty: true },
      },
      t,
    );

    expect(card.status).toBe("degraded");
    expect(card.statusLabel).toBe("reservoir empty");
  });

  it("formats the last fill from milliseconds", () => {
    const value = atoCard(snapshot, t).metrics.find(
      (m) => m.label === "last fill",
    )?.value;

    expect(value).toBe("4s");
  });
});

describe("lightingCard", () => {
  it("reports the profile and both channels", () => {
    const card = lightingCard(snapshot, t);

    expect(card.statusLabel).toBe("daylight");
    expect(card.metrics.find((m) => m.label === "white / rgb")?.value).toBe(
      "60% / 40%",
    );
  });

  it("is degraded while an override is active, so it reads as not on schedule", () => {
    const card = lightingCard(
      {
        ...snapshot,
        lighting: { ...snapshot.lighting, override_active: true },
      },
      t,
    );

    expect(card.status).toBe("degraded");
    expect(card.statusLabel).toBe("override");
  });
});

describe("systemCard", () => {
  it("formats heap in mebibytes once PSRAM is live", () => {
    const value = systemCard(snapshot, t).metrics.find(
      (m) => m.label === "free heap",
    )?.value;

    expect(value).toBe("8.1 MiB");
  });

  it("is degraded outside normal mode", () => {
    expect(
      systemCard({ ...snapshot, system_mode: "maintenance" }, t).status,
    ).toBe("degraded");
  });
});
