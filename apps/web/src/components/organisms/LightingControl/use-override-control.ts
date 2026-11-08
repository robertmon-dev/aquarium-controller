import {
  useClearLightingOverride,
  useLightingOverride,
  useSetLightingOverride,
  type ChannelLevels,
  type LightingMetricsDto,
  type LightingProfile,
  type Transport,
} from "@aquarium/api-client";
import { channelRequest, profileRequest, useCountdown } from "@aquarium/view-model";
import { useReducer, useRef } from "react";
import {
  initialLightingState,
  lightingReducer,
  type LightingState,
} from "./lighting-reducer";

/**
 * The device reports only what is left of an override, never the duration it
 * started with, so the highest value seen during this one is the denominator
 * for the countdown bar. It resets with the override, which is what makes a
 * reload start a fresh scale.
 */
function useCountdownScale(remaining: number | null): number | null {
  const total = useRef<number | null>(null);

  if (remaining === null) total.current = null;
  else if (total.current === null || remaining > total.current)
    total.current = remaining;

  return total.current;
}

export interface OverrideControl {
  state: LightingState;
  remaining: number | null;
  total: number | null;
  busy: boolean;
  active: boolean;
  failure: Error | null;
  /** True while the override drives the channels rather than a profile. */
  manualChannels: boolean;
  selectedProfile: LightingProfile | null;
  setDuration: (seconds: number | null) => void;
  pickProfile: (profile: LightingProfile) => void;
  setChannel: (channel: keyof ChannelLevels, value: number) => void;
  commitChannels: () => void;
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

  // The device keeps naming a scheduled profile even when an override drives
  // the channels directly, so "manual" comes from the override target.
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

  // None of the handlers below are memoised: half of them close over `state`,
  // so a stable identity would hand back a stale duration or dirty flag, and
  // nothing downstream is memoised to benefit from it anyway.
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

    confirmRestore: () => dispatch({ type: "confirmRestore" }),

    cancelRestore: () => dispatch({ type: "cancelRestore" }),

    restore: () =>
      clearOverride.mutate(undefined, {
        onSuccess: () => dispatch({ type: "restore", levels: deviceLevels }),
      }),
  };
}
