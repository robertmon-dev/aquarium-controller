import { z } from "zod";
import type { DeviceConfig } from "./config";

export interface DeviceFormMessages {
  hostRequired: string;
  hostNoScheme: string;
  portNumber: string;
  portRange: string;
  tokenRequired: string;
}

export function deviceFormSchema(m: DeviceFormMessages) {
  return z.object({
    host: z
      .string()
      .trim()
      .min(1, m.hostRequired)
      .refine((value) => !value.includes("://"), m.hostNoScheme),
    port: z
      .string()
      .trim()
      .transform((value) => (value === "" ? "80" : value))
      .refine((value) => /^\d+$/.test(value), m.portNumber)
      .transform(Number)
      .refine((value) => value >= 1 && value <= 65535, m.portRange),
    token: z.string().trim().min(1, m.tokenRequired),
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
