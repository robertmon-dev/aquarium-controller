import { ApiError } from "@aquarium/api-client";
import type { Translate } from "@aquarium/i18n";

export function describeCalibrationFailure(
  error: unknown,
  t: Translate,
): string {
  if (!(error instanceof ApiError)) return t("calibration.saveFailed");

  if (error.kind === "unauthorized") return t("deviceForm.tokenRejected");

  if (error.status === 500) return t("calibration.notStored");

  const detail = error.detail?.trim();

  return detail !== undefined && detail !== "" ? detail : error.message;
}
