import type { DeviceConfig } from "@aquarium/api-client";
import { z } from "zod";
import type { Translate } from "@/i18n";

export function deviceFormSchema(t: Translate) {
  return z.object({
    host: z
      .string()
      .trim()
      .min(1, t("validation.hostRequired"))
      .refine((value) => !value.includes("://"), t("validation.hostNoScheme")),
    port: z
      .string()
      .trim()
      .transform((value) => (value === "" ? "80" : value))
      .refine((value) => /^\d+$/.test(value), t("validation.portNumber"))
      .transform(Number)
      .refine(
        (value) => value >= 1 && value <= 65535,
        t("validation.portRange"),
      ),
    token: z.string().trim().min(1, t("validation.tokenRequired")),
  });
}

export interface DeviceFormValues {
  host: string;
  port: string;
  token: string;
}

export type ParsedDeviceForm = z.output<ReturnType<typeof deviceFormSchema>>;

export function toDeviceConfig(parsed: ParsedDeviceForm): DeviceConfig {
  return {
    host: parsed.host.trim(),
    port: parsed.port,
    token: parsed.token.trim(),
  };
}
