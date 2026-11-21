import type { HeaterId, PumpId } from "./generated";

export interface DeviceConfig {
  host: string;
  port: number;
  token: string;
}

export type ApiPath =
  | "/api/telemetry"
  | "/api/health"
  | "/api/probe"
  | "/api/calibration"
  | "/api/heaters"
  | "/api/pumps"
  | "/api/pumps/settings"
  | `/api/pumps/${PumpId}/override`
  | `/api/pumps/${PumpId}/runtime/reset`
  | "/api/pumps/top-off"
  | "/api/pumps/top-off/resume"
  | "/api/water-change"
  | "/api/water-change/resume"
  | "/api/heaters/settings"
  | `/api/heaters/${HeaterId}/override`
  | "/api/lighting/override"
  | `/api/lighting/schedule/${number}`;

export const EVENTS_PATH = "/api/events";

export function baseUrl(device: DeviceConfig): string {
  return `http://${device.host}:${device.port}`;
}

export function endpointUrl(device: DeviceConfig, path: ApiPath): string {
  return `${baseUrl(device)}${path}`;
}

export function eventsUrl(device: DeviceConfig): string {
  const token = encodeURIComponent(device.token);
  return `${baseUrl(device)}${EVENTS_PATH}?token=${token}`;
}
