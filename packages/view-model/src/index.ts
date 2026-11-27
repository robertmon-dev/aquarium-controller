export type {
  BadgeVariant,
  ChipVariant,
  Metric,
  StatusTone,
  StreamState,
  TextDisplayState,
} from "./semantics";

export {
  EM_DASH,
  formatCelsius,
  formatDuration,
  formatHeap,
  formatLitresPerHour,
  formatPercent,
  formatChannelLevels,
  formatMicrosiemens,
  formatPh,
  formatPpm,
  formatRssi,
} from "./format";

export {
  atoCard,
  co2Card,
  filtrationCard,
  lightingCard,
  pumpsCard,
  RATED_MAX_PPM,
  systemCard,
  tdsCard,
  temperatureCard,
} from "./cards";
export type { CardData, HeroCardData } from "./cards";

export { waterLevelCard } from "./water-level";
export type { WaterLevelCardData, WaterLevelChip } from "./water-level";

export {
  CHANNEL_KEYS,
  channelLabels,
  channelRequest,
  durationChoices,
  LIGHTING_PROFILES,
  profileRequest,
  sumpLightDurations,
  tickCountdown,
} from "./lighting-request";
export type { DurationChoice } from "./lighting-request";

export { streamBadge } from "./stream-badge";

export { useCountdown } from "./use-countdown";

export { initialStreamState, streamReducer } from "./stream-reducer";
export type { StreamAction, StreamStatus } from "./stream-reducer";

export { initialLightingState, lightingReducer } from "./lighting-reducer";
export type {
  LightingAction,
  LightingMode,
  LightingState,
} from "./lighting-reducer";

export { useOverrideControl } from "./use-override-control";
export type { OverrideControl } from "./use-override-control";

export { initialProbe, probeReducer } from "./probe-reducer";
export type { ProbeAction, ProbeState, ProbeTrigger } from "./probe-reducer";
export { describeProbeFailure } from "./probe-failure";
export { useDeviceProbe } from "./use-device-probe";
export type { DeviceProbe } from "./use-device-probe";

export { useTelemetryStream } from "./use-telemetry-stream";

export {
  advanceProbeSamples,
  appendSample,
  canCapturePh,
  canUseTdsReading,
  PROBE_FRESHNESS_SECONDS,
  probeAge,
  probeStability,
  STABILITY_SPREAD_VOLTS,
  STABILITY_WINDOW,
  tdsFactorFromReference,
} from "./probe-calibration";
export type {
  PhCaptureInput,
  ProbeStability,
  TdsFactorInput,
  TdsReferenceInput,
} from "./probe-calibration";

export { useProbeSamples } from "./use-probe-samples";
export type { ProbeSamples, ProbeVoltageReading } from "./use-probe-samples";
export { describeCalibrationFailure } from "./calibration-failure";

export { canResetFlood, floodMessage } from "./flood";
