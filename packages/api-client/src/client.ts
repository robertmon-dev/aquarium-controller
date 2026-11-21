import type {
  Calibration,
  CalibrationPatch,
  ChannelStatus,
  HeaterId,
  HeaterSettings,
  HeaterSettingsPatch,
  HeaterStatus,
  HeatersStatus,
  LightingOverride,
  LightingScheduleDto,
  ProbeVoltagesDto,
  PumpId,
  PumpSettings,
  PumpSettingsPatch,
  PumpsStatus,
  SetHeaterOverrideRequest,
  SetOverrideRequest,
  SetPumpOverrideRequest,
  StartTopOffRequest,
  SystemHealthDto,
  TankTelemetryDto,
  WaterChangePlan,
} from "./generated";
import {
  assertChannelStatus,
  assertPumpSettings,
  assertPumpsStatus,
} from "./schema";
import { request, type Transport } from "./transport";

export type OverridablePump = Extract<PumpId, "return" | "uv">;

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
  pumps: () => Promise<PumpsStatus>;
  pumpSettings: () => Promise<PumpSettings>;
  patchPumpSettings: (body: PumpSettingsPatch) => Promise<PumpSettings>;
  setPumpOverride: (
    id: OverridablePump,
    body: SetPumpOverrideRequest,
  ) => Promise<ChannelStatus>;
  clearPumpOverride: (id: OverridablePump) => Promise<void>;
  resetPumpRuntime: (id: PumpId) => Promise<void>;
  startTopOff: (body: StartTopOffRequest) => Promise<void>;
  resumeTopOff: () => Promise<void>;
  cancelTopOff: () => Promise<void>;
  startWaterChange: (body: WaterChangePlan) => Promise<void>;
  resumeWaterChange: () => Promise<void>;
  cancelWaterChange: () => Promise<void>;
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

    pumps: () =>
      request<unknown>(transport, "/api/pumps").then(assertPumpsStatus),

    pumpSettings: () =>
      request<unknown>(transport, "/api/pumps/settings").then(
        assertPumpSettings,
      ),

    patchPumpSettings: (body) =>
      request<unknown>(transport, "/api/pumps/settings", {
        method: "PATCH",
        body: JSON.stringify(body),
      }).then(assertPumpSettings),

    setPumpOverride: (id, body) =>
      request<unknown>(transport, `/api/pumps/${id}/override`, {
        method: "PUT",
        body: JSON.stringify(body),
      }).then(assertChannelStatus),

    clearPumpOverride: (id) =>
      request<void>(transport, `/api/pumps/${id}/override`, {
        method: "DELETE",
      }),

    resetPumpRuntime: (id) =>
      request<void>(transport, `/api/pumps/${id}/runtime/reset`, {
        method: "POST",
      }),

    startTopOff: (body) =>
      request<void>(transport, "/api/pumps/top-off", {
        method: "POST",
        body: JSON.stringify(body),
      }),

    resumeTopOff: () =>
      request<void>(transport, "/api/pumps/top-off/resume", {
        method: "POST",
      }),

    cancelTopOff: () =>
      request<void>(transport, "/api/pumps/top-off", { method: "DELETE" }),

    startWaterChange: (body) =>
      request<void>(transport, "/api/water-change", {
        method: "POST",
        body: JSON.stringify(body),
      }),

    resumeWaterChange: () =>
      request<void>(transport, "/api/water-change/resume", {
        method: "POST",
      }),

    cancelWaterChange: () =>
      request<void>(transport, "/api/water-change", { method: "DELETE" }),
  };
};
