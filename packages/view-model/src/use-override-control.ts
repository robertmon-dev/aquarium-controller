import {
  useClearLightingOverride,
  useLightingOverride,
  useSetLightingOverride,
  type ChannelLevels,
  type LightingMetricsDto,
  type LightingProfile,
  type Transport,
} from "@aquarium/api-client";
import { channelRequest, profileRequest } from "./lighting-request";
import { useCountdown } from "./use-countdown";
import { useEffect, useReducer, useState } from "react";
import {
  initialLightingState,
  lightingReducer,
  type LightingState,
} from "./lighting-reducer";

function useCountdownScale(remaining: number | null): number | null {
  const [total, setTotal] = useState<number | null>(null);

  useEffect(() => {
    if (remaining === null) {
      setTotal(null);
      return;
    }

    setTotal((current) =>
      current === null || remaining > current ? remaining : current,
    );
  }, [remaining]);

  return total;
}

export interface OverrideControl {
  state: LightingState;
  remaining: number | null;
  total: number | null;
  busy: boolean;
  active: boolean;
  failure: Error | null;
  manualChannels: boolean;
  selectedProfile: LightingProfile | null;
  setDuration: (seconds: number | null) => void;
  pickProfile: (profile: LightingProfile) => void;
  setChannel: (channel: keyof ChannelLevels, value: number) => void;
  commitChannels: () => void;
  /**
   * Sets both channels and sends them in one call. React Native's slider
   * reports only a finished value, so there is no drag to accumulate — and
   * committing from the argument rather than from reducer state avoids reading
   * a `dirty` flag that the same handler just set.
   */
  commitLevels: (levels: ChannelLevels) => void;
  confirmRestore: () => void;
  cancelRestore: () => void;
  restore: () => void;
}

export function useOverrideControl(
  transport: Transport,
  lighting: LightingMetricsDto,
): OverrideControl {
  const override = useLightingOverride(transport);
  const setOverride = useSetLightingOverride(transport);
  const clearOverride = useClearLightingOverride(transport);

  const [state, dispatch] = useReducer(
    lightingReducer,
    {
      white_pct: lighting.channel_white_pct,
      rgb_pct: lighting.channel_rgb_pct,
    },
    initialLightingState,
  );

  const refetch = override.refetch;
  const remaining = useCountdown(override.data?.expires_in_s ?? null, () => {
    void refetch();
  });
  const total = useCountdownScale(remaining);

  const deviceLevels: ChannelLevels = {
    white_pct: lighting.channel_white_pct,
    rgb_pct: lighting.channel_rgb_pct,
  };

  const committed = { onSuccess: () => dispatch({ type: "committed" }) };

  const manualChannels =
    lighting.override_active &&
    ((state.mode === "channels" && state.dirty) ||
      (override.data != null && "channels" in override.data.target));

  const selectedProfile: LightingProfile | null =
    state.mode === "profile" && state.profile
      ? state.profile
      : override.data && "profile" in override.data.target
        ? override.data.target.profile
        : null;

  return {
    state,
    remaining,
    total,
    busy: setOverride.isPending || clearOverride.isPending,
    active: lighting.override_active,
    failure: setOverride.error ?? clearOverride.error,
    manualChannels,
    selectedProfile,

    setDuration: (seconds) => dispatch({ type: "setDuration", seconds }),

    pickProfile: (profile) => {
      dispatch({ type: "pickProfile", profile });
      setOverride.mutate(profileRequest(profile, state.duration), committed);
    },

    setChannel: (channel, value) =>
      dispatch({ type: "setChannel", channel, value }),

    commitChannels: () => {
      if (!state.dirty) return;
      setOverride.mutate(
        channelRequest(state.levels, state.duration),
        committed,
      );
    },

    commitLevels: (levels) => {
      dispatch({
        type: "setChannel",
        channel: "white_pct",
        value: levels.white_pct,
      });
      dispatch({
        type: "setChannel",
        channel: "rgb_pct",
        value: levels.rgb_pct,
      });
      setOverride.mutate(channelRequest(levels, state.duration), committed);
    },

    confirmRestore: () => dispatch({ type: "confirmRestore" }),

    cancelRestore: () => dispatch({ type: "cancelRestore" }),

    restore: () =>
      clearOverride.mutate(undefined, {
        onSuccess: () => dispatch({ type: "restore", levels: deviceLevels }),
      }),
  };
}
