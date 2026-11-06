export type DeviceConfig = {
  host: string;
  port: number;
  token: string;
};

export type ApiPath =
  | "/api/telemetry"
  | "/api/health"
  | "/api/lighting/override";

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
