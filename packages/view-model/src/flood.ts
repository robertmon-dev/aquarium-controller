import type { TankTelemetryDto } from "@aquarium/api-client";
import type { Translate } from "@aquarium/i18n";

type Snapshot = Pick<TankTelemetryDto, "flood" | "leak_detected">;

export function floodMessage(
  { flood, leak_detected }: Snapshot,
  t: Translate,
): string | null {
  const probes = flood?.probes;
  if (probes?.reservoir && probes.sump) return t("telemetry.floodBoth");
  if (probes?.reservoir) return t("telemetry.floodReservoir");
  if (probes?.sump) return t("telemetry.floodSump");
  if (flood?.latched) {
    return t(probes ? "telemetry.floodDry" : "telemetry.floodUnknown");
  }
  return leak_detected ? t("telemetry.leakBody") : null;
}

export function canResetFlood({ flood }: Snapshot): boolean {
  if (!flood?.latched || !flood.probes) return false;
  return !flood.probes.reservoir && !flood.probes.sump;
}
