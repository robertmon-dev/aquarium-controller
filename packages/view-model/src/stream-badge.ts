import type { Translate } from "@aquarium/i18n";
import type { BadgeVariant, StreamState } from "./semantics";

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
