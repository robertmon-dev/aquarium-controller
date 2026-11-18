import type { CalibrationFormMessages } from "@aquarium/api-client";
import type { Translate } from "@aquarium/i18n";

export const calibrationFormMessages = (
  t: Translate,
): CalibrationFormMessages => ({
  numberRequired: t("calibrationValidation.numberRequired"),
  spanTooSmall: t("calibrationValidation.spanTooSmall"),
  makeupRange: t("calibrationValidation.makeupRange"),
  factorRange: t("calibrationValidation.factorRange"),
});
