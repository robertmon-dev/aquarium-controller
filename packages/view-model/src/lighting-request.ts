import type {
  ChannelLevels,
  LightingProfile,
  SetOverrideRequest,
} from "@aquarium/api-client";
import type { Translate } from "@aquarium/i18n";

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

export function durationChoices(t: Translate): DurationChoice[] {
  return [
    { label: t("durations.m5"), seconds: 300 },
    { label: t("durations.m15"), seconds: 900 },
    { label: t("durations.h1"), seconds: 3600 },
    { label: t("durations.none"), seconds: null },
  ];
}

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
