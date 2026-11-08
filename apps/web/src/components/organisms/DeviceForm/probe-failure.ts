import { ApiError, type DeviceConfig } from "@aquarium/api-client";
import type { Translate } from "@aquarium/i18n";

/**
 * Turns whatever the probe threw into the sentence the form shows. Pure, so the
 * mapping is testable without a device or a render.
 */
export function describeProbeFailure(
  error: unknown,
  device: DeviceConfig,
  t: Translate,
): string {
  if (error instanceof ApiError && error.kind === "unauthorized") {
    return t("deviceForm.tokenRejected");
  }

  return t("deviceForm.unreachable", {
    host: device.host,
    port: device.port,
  });
}
