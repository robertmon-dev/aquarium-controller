import { ApiError, type DeviceConfig } from "@aquarium/api-client";
import type { Translate } from "@aquarium/i18n";

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
