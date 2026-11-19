import {
  ApiError,
  assertTelemetry,
  publishTelemetry,
  streamTelemetry,
  type Transport,
} from "@aquarium/api-client";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useReducer } from "react";
import {
  initialStreamState,
  streamReducer,
  type StreamStatus,
} from "./stream-reducer";

const RETRY_MS = 3_000;

export function useTelemetryStream(transport: Transport | null): StreamStatus {
  const queryClient = useQueryClient();
  const [state, dispatch] = useReducer(streamReducer, initialStreamState);

  useEffect(() => {
    dispatch({ type: "reset" });
    if (!transport) return;

    const active = transport;
    const controller = new AbortController();
    let cancelled = false;
    let retryTimer: ReturnType<typeof setTimeout> | undefined;

    async function run() {
      while (!cancelled) {
        dispatch({ type: "connecting" });

        try {
          for await (const raw of streamTelemetry(active, controller.signal)) {
            if (cancelled) return;
            dispatch({ type: "snapshot", at: Date.now() });
            publishTelemetry(queryClient, active.device, assertTelemetry(raw));
          }
          if (cancelled) return;
          dispatch({ type: "dropped" });
        } catch (error) {
          if (cancelled) return;

          if (error instanceof ApiError && error.kind === "unauthorized") {
            dispatch({ type: "unauthorized" });
            return;
          }

          dispatch({ type: "dropped" });
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

  return state;
}
