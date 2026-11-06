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
    const fetchImpl = vi
      .fn()
      .mockResolvedValue(
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
