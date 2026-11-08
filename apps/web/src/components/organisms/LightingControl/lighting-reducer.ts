import type { ChannelLevels, LightingProfile } from "@aquarium/api-client";

export type LightingMode = "schedule" | "profile" | "channels";

export interface LightingState {
  mode: LightingMode;
  profile?: LightingProfile;
  levels: ChannelLevels;
  duration: number | null;
  dirty: boolean;
  confirming: boolean;
}

export type LightingAction =
  | { type: "pickProfile"; profile: LightingProfile }
  | { type: "setChannel"; channel: keyof ChannelLevels; value: number }
  | { type: "setDuration"; seconds: number | null }
  | { type: "restore"; levels: ChannelLevels }
  | { type: "committed" }
  | { type: "confirmRestore" }
  | { type: "cancelRestore" };

const DEFAULT_DURATION_S = 900;

export function initialLightingState(levels: ChannelLevels): LightingState {
  return {
    mode: "schedule",
    levels,
    duration: DEFAULT_DURATION_S,
    dirty: false,
    confirming: false,
  };
}

function clampPct(value: number): number {
  return Math.min(100, Math.max(0, Math.round(value)));
}

export function lightingReducer(
  state: LightingState,
  action: LightingAction,
): LightingState {
  switch (action.type) {
    case "pickProfile":
      return {
        ...state,
        mode: "profile",
        profile: action.profile,
        dirty: true,
      };

    case "setChannel":
      return {
        ...state,
        mode: "channels",
        levels: { ...state.levels, [action.channel]: clampPct(action.value) },
        dirty: true,
      };

    case "setDuration":
      return { ...state, duration: action.seconds };

    case "restore":
      return {
        ...state,
        mode: "schedule",
        levels: action.levels,
        dirty: false,
        confirming: false,
      };

    case "committed":
      return { ...state, dirty: false };

    case "confirmRestore":
      return { ...state, confirming: true };

    case "cancelRestore":
      return { ...state, confirming: false };
  }
}
