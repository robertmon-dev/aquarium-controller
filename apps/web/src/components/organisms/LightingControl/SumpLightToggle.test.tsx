import type { SumpLightStatus, Transport } from "@aquarium/api-client";
import { memoryAdapter } from "@aquarium/i18n/adapter";
import { LocaleProvider } from "@aquarium/i18n/use-locale";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SumpLightToggle } from "./SumpLightToggle";

function renderToggle(status: SumpLightStatus) {
  const fetchImpl = vi.fn(() =>
    Promise.resolve(
      new Response(JSON.stringify(status), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    ),
  );
  const transport: Transport = {
    device: { host: "10.0.0.2", port: 80, token: "tok" },
    fetchImpl,
  };
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  render(
    <QueryClientProvider client={client}>
      <LocaleProvider adapter={memoryAdapter({ stored: "en", system: "en" })}>
        <SumpLightToggle transport={transport} status={status} />
      </LocaleProvider>
    </QueryClientProvider>,
  );
}

describe("SumpLightToggle", () => {
  it("does not offer switching on while the control is unavailable", async () => {
    renderToggle({ on: false, available: false, expires_in_s: null });

    const button = await screen.findByRole("button", { name: "turn on" });
    await waitFor(() => expect(button).toHaveProperty("disabled", true));
  });

  it("still allows switching off while the control is unavailable", async () => {
    renderToggle({ on: true, available: false, expires_in_s: null });

    const button = await screen.findByRole("button", { name: "turn off" });
    expect(button).toHaveProperty("disabled", false);
  });
});
