import type { Transport } from "@aquarium/api-client";
import { memoryAdapter } from "@aquarium/i18n/adapter";
import { LocaleProvider } from "@aquarium/i18n/use-locale";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CalibrationPanel } from "./CalibrationPanel";

const device = { host: "10.0.0.2", port: 80, token: "tok" };

const calibration = {
  ph: { neutral_volts: 1.5, acid_volts: 1.67748, calibrated_at_celsius: 25 },
  makeup_tank_celsius: 25,
  tds_factor: 1,
};

function json(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

function transportWith(probeVolts: number | null): {
  transport: Transport;
  fetchImpl: ReturnType<typeof vi.fn>;
} {
  const fetchImpl = vi.fn((url: string) => {
    if (url.endsWith("/api/probe")) {
      return Promise.resolve(
        json({ ph: probeVolts, tds: 0.9, sampled_at_epoch_s: 1_750_000_000 }),
      );
    }

    if (url.endsWith("/api/calibration")) {
      return Promise.resolve(json(calibration));
    }

    return Promise.resolve(
      json({
        timestamp_epoch_s: 1_750_000_000,
        co2: {
          current_ph: 6.85,
          ph_electrode_health: 0.94,
          solenoid_open: false,
          drop_checker_ph_target: null,
        },
        tds: { ppm: 400, ec_us_cm: 800 },
      }),
    );
  });

  return { transport: { device, fetchImpl }, fetchImpl };
}

function liveDevice() {
  let frozenAt: number | null = null;

  const fetchImpl = vi.fn((url: string) => {
    if (url.endsWith("/api/probe")) {
      const stamp = frozenAt ?? Math.floor(Date.now() / 1000);
      return Promise.resolve(
        json({ ph: 1.52, tds: 0.9, sampled_at_epoch_s: stamp }),
      );
    }

    return transportWith(1.52).transport.fetchImpl(url);
  });

  return {
    transport: { device, fetchImpl },
    freeze: (offsetSeconds = 0) => {
      frozenAt = Math.floor(Date.now() / 1000) + offsetSeconds;
    },
  };
}

async function advance(seconds: number) {
  for (let i = 0; i < seconds; i++) {
    await act(() => vi.advanceTimersByTimeAsync(1_000));
  }
}

function neutralInput(): HTMLInputElement {
  const input = document.querySelector<HTMLInputElement>(
    'input[name="neutralVolts"]',
  );
  if (!input) throw new Error("no neutral volts input");
  return input;
}

function renderPanel(transport: Transport) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  return render(
    <QueryClientProvider client={client}>
      <LocaleProvider adapter={memoryAdapter({ stored: "en", system: "en" })}>
        <CalibrationPanel transport={transport} />
      </LocaleProvider>
    </QueryClientProvider>,
  );
}

describe("CalibrationPanel", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("captures a stable reading the device is still refreshing", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { transport } = liveDevice();

    renderPanel(transport);
    fireEvent.click(await screen.findByText("calibrate probes"));
    await advance(8);

    fireEvent.click(screen.getByText("capture as 7.00"));

    await waitFor(() => expect(neutralInput().value).toBe("1.5200"));
  });

  it("refuses to capture a stable reading that went stale before the click", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { transport, freeze } = liveDevice();

    renderPanel(transport);
    fireEvent.click(await screen.findByText("calibrate probes"));
    await advance(8);

    freeze();
    await advance(15);

    fireEvent.click(screen.getByText("capture as 7.00"));
    await advance(1);

    expect(neutralInput().value).not.toBe("1.5200");
  });

  it("discards a future-dated reading even after the clock catches up", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const { transport, freeze } = liveDevice();
    renderPanel(transport);
    fireEvent.click(await screen.findByText("calibrate probes"));
    await advance(8);

    freeze(5);
    await advance(6);
    fireEvent.click(screen.getByText("capture as 7.00"));

    expect(neutralInput().value).not.toBe("1.5200");
    expect(screen.getByText("capture as 7.00").closest("button")?.disabled).toBe(
      true,
    );
  });

  it("does not poll the probe while it is collapsed", async () => {
    const { transport, fetchImpl } = transportWith(1.52);

    renderPanel(transport);

    await waitFor(() =>
      expect(screen.getByText("calibrate probes")).toBeTruthy(),
    );

    const probed = fetchImpl.mock.calls.filter(([url]) =>
      String(url).endsWith("/api/probe"),
    );
    expect(probed).toHaveLength(0);
  });

  it("shows the live probe voltage once expanded", async () => {
    const { transport } = transportWith(1.52);

    renderPanel(transport);
    fireEvent.click(await screen.findByText("calibrate probes"));

    await waitFor(() => expect(screen.getByText("1.5200 V")).toBeTruthy());
  });

  it("refuses to capture a point while the reading is still settling", async () => {
    const { transport } = transportWith(1.52);

    renderPanel(transport);
    fireEvent.click(await screen.findByText("calibrate probes"));

    const capture = await screen.findByText("capture as 7.00");

    await waitFor(() =>
      expect(screen.getAllByText("settling…")).toHaveLength(2),
    );
    expect(capture.closest("button")?.disabled).toBe(true);
  });

  it("marks a reading the device stopped refreshing as stale", async () => {
    const { transport } = transportWith(1.52);

    renderPanel(transport);
    fireEvent.click(await screen.findByText("calibrate probes"));

    await waitFor(() =>
      expect(screen.getAllByText(/last read \d+s ago/).length).toBeGreaterThan(
        0,
      ),
    );
  });

  it("reports a disconnected probe instead of showing a voltage", async () => {
    const { transport } = transportWith(null);

    renderPanel(transport);
    fireEvent.click(await screen.findByText("calibrate probes"));

    await waitFor(() =>
      expect(screen.getByText("probe not connected")).toBeTruthy(),
    );
  });
});
