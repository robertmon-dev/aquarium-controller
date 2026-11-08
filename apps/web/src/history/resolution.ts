export const SAMPLE_INTERVAL_S = 2;
export const RAW_LIMIT_S = 24 * 3600;
export const MAX_POINTS = 5000;

export type Resolution =
  | { mode: "raw" }
  | { mode: "aggregated"; everySeconds: number };

export function resolutionFor(
  rangeSeconds: number,
  forceRaw: boolean,
): Resolution {
  if (forceRaw || rangeSeconds <= RAW_LIMIT_S) return { mode: "raw" };

  const every = Math.ceil(rangeSeconds / MAX_POINTS);
  return {
    mode: "aggregated",
    everySeconds: Math.max(SAMPLE_INTERVAL_S, every),
  };
}
