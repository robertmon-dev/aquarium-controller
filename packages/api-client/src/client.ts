import type {
  Calibration,
  CalibrationPatch,
  HeaterId,
  HeaterSettings,
  HeaterSettingsPatch,
  HeaterStatus,
  HeatersStatus,
  LightingOverride,
  LightingScheduleDto,
  ProbeVoltagesDto,
  SetHeaterOverrideRequest,
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
  lightingSchedule: (month: number) => Promise<LightingScheduleDto>;
  probeVoltages: () => Promise<ProbeVoltagesDto>;
  calibration: () => Promise<Calibration>;
  patchCalibration: (body: CalibrationPatch) => Promise<Calibration>;
  heaters: () => Promise<HeatersStatus>;
  heaterSettings: () => Promise<HeaterSettings>;
  patchHeaterSettings: (body: HeaterSettingsPatch) => Promise<HeaterSettings>;
  setHeaterOverride: (
    id: HeaterId,
    body: SetHeaterOverrideRequest,
  ) => Promise<HeaterStatus>;
  clearHeaterOverride: (id: HeaterId) => Promise<void>;
}

export const createClient = (transport: Transport): DeviceClient => {
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

    lightingSchedule: (month) =>
      request<LightingScheduleDto>(
        transport,
        `/api/lighting/schedule/${month}`,
      ),

    probeVoltages: () => request<ProbeVoltagesDto>(transport, "/api/probe"),

    calibration: () => request<Calibration>(transport, "/api/calibration"),

    patchCalibration: (body) =>
      request<Calibration>(transport, "/api/calibration", {
        method: "PATCH",
        body: JSON.stringify(body),
      }),

    heaters: () => request<HeatersStatus>(transport, "/api/heaters"),

    heaterSettings: () =>
      request<HeaterSettings>(transport, "/api/heaters/settings"),

    patchHeaterSettings: (body) =>
      request<HeaterSettings>(transport, "/api/heaters/settings", {
        method: "PATCH",
        body: JSON.stringify(body),
      }),

    setHeaterOverride: (id, body) =>
      request<HeaterStatus>(transport, `/api/heaters/${id}/override`, {
        method: "PUT",
        body: JSON.stringify(body),
      }),

    clearHeaterOverride: (id) =>
      request<void>(transport, `/api/heaters/${id}/override`, {
        method: "DELETE",
      }),
  };
};
