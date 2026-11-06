import type { BadgeVariant } from "@aquarium/theme";
import type { StreamState } from "@/device/use-telemetry-stream";

const BADGES: Record<StreamState, { label: string; variant: BadgeVariant }> = {
  idle: { label: "not connected", variant: "outline" },
  connecting: { label: "connecting…", variant: "info" },
  live: { label: "live", variant: "success" },
  reconnecting: { label: "reconnecting…", variant: "warning" },
  unauthorized: { label: "token rejected", variant: "danger" },
};

export function streamBadge(state: StreamState): {
  label: string;
  variant: BadgeVariant;
} {
  return BADGES[state];
}
