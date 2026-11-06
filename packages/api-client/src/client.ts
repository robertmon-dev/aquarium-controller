import type {
  LightingOverride,
  SetOverrideRequest,
  SystemHealthDto,
  TankTelemetryDto,
} from "./generated";
import { request, type Transport } from "./transport";

export interface DeviceClient {
  telemetry: () => Promise<TankTelemetryDto>;
  health: () => Promise<SystemHealthDto>;
  lightingOverride: () => Promise<LightingOverride | null>;
  setLightingOverride: (body: SetOverrideRequest) => Promise<LightingOverride>;
  clearLightingOverride: () => Promise<void>;
}

export function createClient(transport: Transport): DeviceClient {
  return {
    telemetry: () => request<TankTelemetryDto>(transport, "/api/telemetry"),

    health: () => request<SystemHealthDto>(transport, "/api/health"),

    lightingOverride: async () => {
      const over = await request<LightingOverride | undefined>(
        transport,
        "/api/lighting/override",
      );
      return over ?? null;
    },

    setLightingOverride: (body) =>
      request<LightingOverride>(transport, "/api/lighting/override", {
        method: "PUT",
        body: JSON.stringify(body),
      }),

    clearLightingOverride: () =>
      request<void>(transport, "/api/lighting/override", { method: "DELETE" }),
  };
}
