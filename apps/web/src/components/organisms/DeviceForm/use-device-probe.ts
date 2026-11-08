import { createClient, type DeviceConfig } from "@aquarium/api-client";
import { useT } from "@aquarium/i18n/use-locale";
import { useCallback, useReducer } from "react";
import { deviceFetch } from "@/device/transport";
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

export function useDeviceProbe(): DeviceProbe {
  const t = useT();
  const [state, dispatch] = useReducer(probeReducer, initialProbe);

  const run = useCallback(
    async (action: ProbeTrigger, device: DeviceConfig) => {
      dispatch({ type: "start", action });

      try {
        await createClient({ device, fetchImpl: deviceFetch }).health();
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
    [t],
  );

  const dismiss = useCallback(() => dispatch({ type: "dismiss" }), []);

  return { state, probing: state.status === "probing", run, dismiss };
}
