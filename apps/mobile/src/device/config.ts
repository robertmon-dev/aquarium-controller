import type { DeviceConfig } from "@aquarium/api-client";
import { z } from "zod";

export const deviceFormSchema = z.object({
  host: z
    .string()
    .trim()
    .min(1, "enter the device address")
    .refine(
      (value) => !value.includes("://"),
      "drop the scheme, just the host or ip",
    ),
  port: z
    .string()
    .trim()
    .transform((value) => (value === "" ? "80" : value))
    .refine((value) => /^\d+$/.test(value), "port must be a number")
    .transform(Number)
    .refine(
      (value) => value >= 1 && value <= 65535,
      "port must be between 1 and 65535",
    ),
  token: z.string().trim().min(1, "enter the api token"),
});

export interface DeviceFormValues {
  host: string;
  port: string;
  token: string;
}

export type ParsedDeviceForm = z.output<typeof deviceFormSchema>;

export function toDeviceConfig(parsed: ParsedDeviceForm): DeviceConfig {
  return {
    host: parsed.host.trim(),
    port: parsed.port,
    token: parsed.token.trim(),
  };
}
