import { describe, expect, it, vi } from "vitest";
import { createClient } from "./client";

const device = { host: "10.0.0.2", port: 80, token: "tok" };

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

describe("createClient", () => {
  it("starts a water change and treats 202 as done", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 202 }));
    const client = createClient({ device, fetchImpl });

    await client.startWaterChange({
      drain_s: 300,
      settle_s: 30,
      refill_s: 420,
    });

    const [url, init] = fetchImpl.mock.calls[0]!;
    expect(url).toBe("http://10.0.0.2:80/api/water-change");
    expect(init.method).toBe("POST");
    expect(JSON.parse(init.body)).toEqual({
      drain_s: 300,
      settle_s: 30,
      refill_s: 420,
    });
  });

  it("puts a pump override on the pump's own path", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      jsonResponse({
        on: false,
        mode: "manual_off",
        override_expires_in_s: null,
        fault: null,
        blocked_by: null,
        runtime_s: 0,
        runtime_reset_at: null,
      }),
    );
    const client = createClient({ device, fetchImpl });

    await client.setPumpOverride("uv", { duration_s: null });

    const [url, init] = fetchImpl.mock.calls[0]!;
    expect(url).toBe("http://10.0.0.2:80/api/pumps/uv/override");
    expect(init.method).toBe("PUT");
  });

  it("cancels a top-off and resets a runtime counter", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }));
    const client = createClient({ device, fetchImpl });

    await client.cancelTopOff();
    await client.resetPumpRuntime("uv");

    expect(fetchImpl.mock.calls[0]![0]).toBe(
      "http://10.0.0.2:80/api/pumps/top-off",
    );
    expect(fetchImpl.mock.calls[0]![1].method).toBe("DELETE");
    expect(fetchImpl.mock.calls[1]![0]).toBe(
      "http://10.0.0.2:80/api/pumps/uv/runtime/reset",
    );
    expect(fetchImpl.mock.calls[1]![1].method).toBe("POST");
  });

  it("gets a telemetry snapshot", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(jsonResponse({ timestamp_epoch_s: 42 }));
    const client = createClient({ device, fetchImpl });

    const snapshot = await client.telemetry();

    expect(fetchImpl.mock.calls[0]![0]).toBe(
      "http://10.0.0.2:80/api/telemetry",
    );
    expect(snapshot.timestamp_epoch_s).toBe(42);
  });

  it("gets system health", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      jsonResponse({
        uptime_seconds: 10,
        wifi_rssi_dbm: -55,
        free_heap_kb: 120,
      }),
    );
    const client = createClient({ device, fetchImpl });

    await expect(client.health()).resolves.toEqual({
      uptime_seconds: 10,
      wifi_rssi_dbm: -55,
      free_heap_kb: 120,
    });
  });

  it("returns null when no lighting override is active", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }));
    const client = createClient({ device, fetchImpl });

    await expect(client.lightingOverride()).resolves.toBeNull();
  });

  it("puts a channels override with duration_s, not expires_in_s", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      jsonResponse({
        target: { channels: { white_pct: 20, rgb_pct: 10 } },
        expires_in_s: 600,
      }),
    );
    const client = createClient({ device, fetchImpl });

    await client.setLightingOverride({
      target: { channels: { white_pct: 20, rgb_pct: 10 } },
      duration_s: 600,
    });

    const [url, init] = fetchImpl.mock.calls[0]!;
    expect(url).toBe("http://10.0.0.2:80/api/lighting/override");
    expect(init.method).toBe("PUT");
    expect(JSON.parse(init.body)).toEqual({
      target: { channels: { white_pct: 20, rgb_pct: 10 } },
      duration_s: 600,
    });
  });

  it("puts a profile override", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(
        jsonResponse({ target: { profile: "moonlight" }, expires_in_s: null }),
      );
    const client = createClient({ device, fetchImpl });

    await client.setLightingOverride({
      target: { profile: "moonlight" },
      duration_s: null,
    });

    expect(JSON.parse(fetchImpl.mock.calls[0]![1].body).target).toEqual({
      profile: "moonlight",
    });
  });

  it("deletes the override", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }));
    const client = createClient({ device, fetchImpl });

    await client.clearLightingOverride();

    expect(fetchImpl.mock.calls[0]![1].method).toBe("DELETE");
  });
});

describe("createClient calibration", () => {
  it("gets the probe voltages", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(
        jsonResponse({ ph: 1.52, tds: 0.91, sampled_at_epoch_s: 7 }),
      );
    const client = createClient({ device, fetchImpl });

    const probes = await client.probeVoltages();

    expect(fetchImpl.mock.calls[0]![0]).toBe("http://10.0.0.2:80/api/probe");
    expect(probes.ph).toBe(1.52);
  });

  it("gets the stored calibration", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      jsonResponse({
        ph: { neutral_volts: 1.5, acid_volts: 1.6, calibrated_at_celsius: 25 },
        makeup_tank_celsius: 22,
        tds_factor: 1.2,
      }),
    );
    const client = createClient({ device, fetchImpl });

    const calibration = await client.calibration();

    expect(fetchImpl.mock.calls[0]![0]).toBe(
      "http://10.0.0.2:80/api/calibration",
    );
    expect(calibration.tds_factor).toBe(1.2);
  });

  it("patches the calibration with PATCH, not PUT", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      jsonResponse({
        ph: { neutral_volts: 1.5, acid_volts: 1.6, calibrated_at_celsius: 25 },
        makeup_tank_celsius: 22,
        tds_factor: 1,
      }),
    );
    const client = createClient({ device, fetchImpl });

    await client.patchCalibration({
      ph: null,
      makeup_tank_celsius: 22,
      tds_factor: null,
    });

    const init = fetchImpl.mock.calls[0]![1] as RequestInit;
    expect(init.method).toBe("PATCH");
    expect(String(init.body)).toContain("makeup_tank_celsius");
  });

  it("gets the heater status", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(jsonResponse({ tank: {}, sump: {} }));
    const client = createClient({ device, fetchImpl });

    await client.heaters();

    expect(fetchImpl.mock.calls[0]![0]).toBe("http://10.0.0.2:80/api/heaters");
  });

  it("patches heater settings", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse({}));
    const client = createClient({ device, fetchImpl });

    await client.patchHeaterSettings({
      tank: null,
      sump: null,
      cutoff_celsius: 29.5,
    });

    const [url, init] = fetchImpl.mock.calls[0]!;
    expect(url).toBe("http://10.0.0.2:80/api/heaters/settings");
    expect(init.method).toBe("PATCH");
    expect(JSON.parse(init.body)).toEqual({
      tank: null,
      sump: null,
      cutoff_celsius: 29.5,
    });
  });

  it("puts a heater override on the heater's own path", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse({}));
    const client = createClient({ device, fetchImpl });

    await client.setHeaterOverride("sump", { state: "on", duration_s: 600 });

    const [url, init] = fetchImpl.mock.calls[0]!;
    expect(url).toBe("http://10.0.0.2:80/api/heaters/sump/override");
    expect(init.method).toBe("PUT");
    expect(JSON.parse(init.body)).toEqual({ state: "on", duration_s: 600 });
  });

  it("deletes a heater override", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }));
    const client = createClient({ device, fetchImpl });

    await client.clearHeaterOverride("tank");

    const [url, init] = fetchImpl.mock.calls[0]!;
    expect(url).toBe("http://10.0.0.2:80/api/heaters/tank/override");
    expect(init.method).toBe("DELETE");
  });

  const channel = {
    on: true,
    mode: "auto",
    override_expires_in_s: null,
    fault: null,
    blocked_by: null,
    runtime_s: 12,
    runtime_reset_at: null,
  };

  const pumpsStatus = {
    channels: {
      return: channel,
      uv: channel,
      makeup: { ...channel, on: false, blocked_by: "interlock" },
      drain: { ...channel, on: true, fault: "output_error" },
    },
    operation: {
      kind: "water_change",
      state: "refilling",
      phase_remaining_s: 40,
      drain_ran_s: 60,
      settle_ran_s: 30,
      refill_ran_s: 0,
      drained_l: 25,
      refilled_l: null,
    },
    last_top_off: null,
    last_water_change: {
      kind: "water_change",
      started_at: null,
      drain_ran_s: 60,
      settle_ran_s: 30,
      refill_ran_s: 250,
      result: "sump_full",
      resumed: false,
      finished_at: 1700000900,
    },
    last_skipped: {
      kind: "top_off",
      index: 0,
      reason: "storage",
      blocked_by: null,
      at: 1700000000,
    },
    circulation_starved: false,
    level_sensors_suspect: true,
    return_held_after_drain: true,
    invalidation_pending: false,
    supervision: "active",
  };

  const pumpSettings = {
    top_off: [{ cron: "0 8 * * *", duration_s: 60 }],
    water_change: [
      { cron: "30 2 * * sun", drain_s: 112, settle_s: 30, refill_s: 600 },
    ],
    flow_lph: { makeup: 600, drain: null },
    resume_window_s: 600,
    refill_stop_sensor: "sump_return_level",
    min_water_change_gap_h: 12,
    reservoir_ready_l: null,
  };

  const noPatch = {
    top_off: null,
    water_change: null,
    flow_lph: null,
    resume_window_s: null,
    min_water_change_gap_h: null,
  };

  it("accepts a well-formed pump status and settings", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(pumpsStatus))
      .mockResolvedValueOnce(jsonResponse(pumpSettings))
      .mockResolvedValueOnce(jsonResponse(pumpSettings));
    const client = createClient({ device, fetchImpl });

    await expect(client.pumps()).resolves.toEqual(pumpsStatus);
    await expect(client.pumpSettings()).resolves.toEqual(pumpSettings);
    await expect(
      client.patchPumpSettings({ ...noPatch, resume_window_s: 600 }),
    ).resolves.toEqual(pumpSettings);
  });

  it.each([
    [
      "pump status",
      (client: ReturnType<typeof createClient>) => client.pumps(),
    ],
    [
      "pump settings",
      (client: ReturnType<typeof createClient>) => client.pumpSettings(),
    ],
    [
      "patched pump settings",
      (client: ReturnType<typeof createClient>) =>
        client.patchPumpSettings(noPatch),
    ],
  ])("rejects an empty object as %s", async (_label, call) => {
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse({}));
    const client = createClient({ device, fetchImpl });

    await expect(call(client)).rejects.toMatchObject({ kind: "malformed" });
  });

  it("rejects a pump status without the supervision state", async () => {
    const { supervision, ...rest } = pumpsStatus;
    void supervision;
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse(rest));
    const client = createClient({ device, fetchImpl });

    await expect(client.pumps()).rejects.toMatchObject({
      kind: "malformed",
      detail: expect.stringContaining("supervision"),
    });
  });

  it("accepts a history record from firmware without an end time", async () => {
    const { finished_at, ...record } = pumpsStatus.last_water_change;
    void finished_at;
    const status = {
      ...pumpsStatus,
      last_water_change: record,
      channels: {
        ...pumpsStatus.channels,
        drain: { ...channel, blocked_by: "supervision_unavailable" },
      },
    };
    const fetchImpl = vi.fn().mockResolvedValue(jsonResponse(status));
    const client = createClient({ device, fetchImpl });

    await expect(client.pumps()).resolves.toEqual(status);
  });

  it("names the malformed pump status fields", async () => {
    const fetchImpl = vi.fn().mockResolvedValue(
      jsonResponse({
        ...pumpsStatus,
        channels: {
          ...pumpsStatus.channels,
          drain: { ...channel, fault: "on_fire" },
        },
      }),
    );
    const client = createClient({ device, fetchImpl });

    await expect(client.pumps()).rejects.toMatchObject({
      kind: "malformed",
      detail: expect.stringContaining("channels.drain.fault"),
    });
  });
});
