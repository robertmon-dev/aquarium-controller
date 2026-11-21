export type ProbeStability = "stable" | "settling" | "empty";

export const STABILITY_WINDOW = 5;
export const STABILITY_SPREAD_VOLTS = 0.002;
export const PROBE_FRESHNESS_SECONDS = 10;

export interface ProbeVoltageReading {
  ph: number | null;
  tds: number | null;
  sampled_at_epoch_s: number;
}

export interface ProbeSamples {
  ph: number[];
  tds: number[];
}

export function probeStability(samples: readonly number[]): ProbeStability {
  const usable = samples.filter((value) => Number.isFinite(value));

  if (usable.length === 0) return "empty";
  if (usable.length < STABILITY_WINDOW) return "settling";

  const window = usable.slice(-STABILITY_WINDOW);
  const spread = Math.max(...window) - Math.min(...window);

  return spread <= STABILITY_SPREAD_VOLTS ? "stable" : "settling";
}

export interface TdsFactorInput {
  reportedPpm: number;
  currentFactor: number;
  referencePpm: number;
}

export function tdsFactorFromReference({
  reportedPpm,
  currentFactor,
  referencePpm,
}: TdsFactorInput): number | null {
  if (
    !Number.isFinite(reportedPpm) ||
    !Number.isFinite(currentFactor) ||
    !Number.isFinite(referencePpm)
  ) {
    return null;
  }

  if (reportedPpm <= 0 || currentFactor <= 0 || referencePpm <= 0) return null;

  return (currentFactor * referencePpm) / reportedPpm;
}

export function probeAge(
  sampledAtEpochS: number,
  nowEpochS: number,
): number | null {
  if (!Number.isFinite(sampledAtEpochS) || !Number.isFinite(nowEpochS)) {
    return null;
  }

  if (sampledAtEpochS <= 0) return null;

  return Math.max(0, Math.round(nowEpochS - sampledAtEpochS));
}

export function appendSample(
  samples: readonly number[],
  value: number | null,
  window: number = STABILITY_WINDOW,
): number[] {
  if (value === null || !Number.isFinite(value)) return [];

  return [...samples, value].slice(-window);
}

export function advanceProbeSamples(
  previous: ProbeSamples,
  reading: ProbeVoltageReading,
  previousStampS: number | null,
): ProbeSamples {
  const gapped =
    previousStampS !== null &&
    reading.sampled_at_epoch_s - previousStampS > PROBE_FRESHNESS_SECONDS;

  return {
    ph: appendSample(gapped ? [] : previous.ph, reading.ph),
    tds: appendSample(gapped ? [] : previous.tds, reading.tds),
  };
}

export interface PhCaptureInput {
  stability: ProbeStability;
  volts: number | null;
  ageSeconds: number | null;
}

export function canCapturePh({
  stability,
  volts,
  ageSeconds,
}: PhCaptureInput): boolean {
  return (
    stability === "stable" &&
    volts !== null &&
    ageSeconds !== null &&
    ageSeconds <= PROBE_FRESHNESS_SECONDS
  );
}
