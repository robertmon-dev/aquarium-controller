import type { DeviceFormMessages } from "@aquarium/api-client";
import type { Translate } from "@aquarium/i18n";

export function deviceFormMessages(t: Translate): DeviceFormMessages {
  return {
    hostRequired: t("validation.hostRequired"),
    hostNoScheme: t("validation.hostNoScheme"),
    portNumber: t("validation.portNumber"),
    portRange: t("validation.portRange"),
    tokenRequired: t("validation.tokenRequired"),
  };
}
