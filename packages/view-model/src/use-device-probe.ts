import {
  createClient,
  type DeviceConfig,
  type FetchImpl,
} from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { useCallback, useReducer } from "react";
import { describeProbeFailure } from "./probe-failure";
import {
  initialProbe,
  probeReducer,
  type ProbeState,
  type ProbeTrigger,
} from "./probe-reducer";

export interface DeviceProbe {
  state: ProbeState;
  probing: boolean;
  run: (action: ProbeTrigger, device: DeviceConfig) => Promise<boolean>;
  dismiss: () => void;
}

/**
 * `fetchImpl` is injected rather than reached for: the web app uses
 * `globalThis.fetch` and the mobile app uses `expo/fetch`, so a global here
 * would quietly bypass what one of them chose.
 */
export function useDeviceProbe(fetchImpl: FetchImpl): DeviceProbe {
  const t = useT();
  const [state, dispatch] = useReducer(probeReducer, initialProbe);

  const run = useCallback(
    async (action: ProbeTrigger, device: DeviceConfig) => {
      dispatch({ type: "start", action });

      try {
        await createClient({ device, fetchImpl }).health();
        dispatch({ type: "reachable" });
        return true;
      } catch (error) {
        dispatch({
          type: "failed",
          message: describeProbeFailure(error, device, t),
        });
        return false;
      }
    },
    [t, fetchImpl],
  );

  const dismiss = useCallback(() => dispatch({ type: "dismiss" }), []);

  return { state, probing: state.status === "probing", run, dismiss };
}
