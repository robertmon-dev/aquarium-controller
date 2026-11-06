import type {
  ChannelLevels,
  LightingProfile,
  SetOverrideRequest,
} from "@aquarium/api-client";

export const LIGHTING_PROFILES: LightingProfile[] = [
  "off",
  "sunrise",
  "daylight",
  "sunset",
  "moonlight",
];

export interface DurationChoice {
  label: string;
  seconds: number | null;
}

export const DURATION_CHOICES: DurationChoice[] = [
  { label: "5 min", seconds: 300 },
  { label: "15 min", seconds: 900 },
  { label: "1 h", seconds: 3600 },
  { label: "no limit", seconds: null },
];

export function profileRequest(
  profile: LightingProfile,
  duration: number | null,
): SetOverrideRequest {
  return { target: { profile }, duration_s: duration };
}

function clampPct(value: number): number {
  return Math.min(100, Math.max(0, Math.round(value)));
}

export function channelRequest(
  levels: ChannelLevels,
  duration: number | null,
): SetOverrideRequest {
  return {
    target: {
      channels: {
        white_pct: clampPct(levels.white_pct),
        rgb_pct: clampPct(levels.rgb_pct),
      },
    },
    duration_s: duration,
  };
}

export function tickCountdown(current: number | null): {
  next: number | null;
  elapsed: boolean;
} {
  if (current === null) return { next: null, elapsed: false };
  if (current <= 0) return { next: 0, elapsed: false };

  const next = current - 1;
  return { next, elapsed: next <= 0 };
}
