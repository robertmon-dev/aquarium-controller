import type { BadgeVariant } from "@aquarium/theme";
import type { Translate } from "@/i18n";
import type { StreamState } from "@/device/use-telemetry-stream";

const VARIANTS: Record<StreamState, BadgeVariant> = {
  idle: "outline",
  connecting: "info",
  live: "success",
  reconnecting: "warning",
  unauthorized: "danger",
};

export function streamBadge(
  state: StreamState,
  t: Translate,
): { label: string; variant: BadgeVariant } {
  return { label: t(`stream.${state}`), variant: VARIANTS[state] };
}
