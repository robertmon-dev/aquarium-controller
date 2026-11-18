import type { CalibrationFormMessages } from "@aquarium/api-client";
import type { Translate } from "@aquarium/i18n";

export function calibrationFormMessages(t: Translate): CalibrationFormMessages {
  return {
    numberRequired: t("calibrationValidation.numberRequired"),
    spanTooSmall: t("calibrationValidation.spanTooSmall"),
    makeupRange: t("calibrationValidation.makeupRange"),
    factorRange: t("calibrationValidation.factorRange"),
  };
}
