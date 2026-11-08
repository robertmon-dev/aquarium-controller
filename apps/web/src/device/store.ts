import type { DeviceConfig } from "@aquarium/api-client";

const KEY = "aquarium.device";

export async function loadDevice(): Promise<DeviceConfig | null> {
  const raw = localStorage.getItem(KEY);
  if (!raw) return null;

  try {
    return JSON.parse(raw) as DeviceConfig;
  } catch {
    return null;
  }
}

export async function saveDevice(device: DeviceConfig): Promise<void> {
  localStorage.setItem(KEY, JSON.stringify(device));
}

export async function clearDevice(): Promise<void> {
  localStorage.removeItem(KEY);
}
