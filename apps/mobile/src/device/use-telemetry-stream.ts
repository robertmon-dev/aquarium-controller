import {
  ApiError,
  assertTelemetry,
  publishTelemetry,
  streamTelemetry,
  type Transport,
} from "@aquarium/api-client";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";

export type StreamState =
  | "idle"
  | "connecting"
  | "live"
  | "reconnecting"
  | "unauthorized";

const RETRY_MS = 3_000;

export function useTelemetryStream(transport: Transport | null): StreamState {
  const queryClient = useQueryClient();
  const [state, setState] = useState<StreamState>("connecting");

  useEffect(() => {
    if (!transport) return;

    const active = transport;
    const controller = new AbortController();
    let cancelled = false;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;

    async function run() {
      while (!cancelled) {
        try {
          for await (const raw of streamTelemetry(active, controller.signal)) {
            if (cancelled) return;
            setState("live");
            publishTelemetry(queryClient, active.device, assertTelemetry(raw));
          }
          if (cancelled) return;
          setState("reconnecting");
        } catch (error) {
          if (cancelled) return;

          if (error instanceof ApiError && error.kind === "unauthorized") {
            setState("unauthorized");
            return;
          }

          setState("reconnecting");
        }

        await new Promise<void>((resolve) => {
          retryTimer = setTimeout(resolve, RETRY_MS);
        });
      }
    }

    void run();

    return () => {
      cancelled = true;
      controller.abort();
      if (retryTimer) clearTimeout(retryTimer);
    };
  }, [transport, queryClient]);

  return transport ? state : "idle";
}
