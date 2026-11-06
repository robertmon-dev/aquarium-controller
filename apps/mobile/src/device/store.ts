import type { DeviceConfig } from "@aquarium/api-client";
import * as SecureStore from "expo-secure-store";

const KEY = "aquarium.device";

export async function loadDevice(): Promise<DeviceConfig | null> {
  const raw = await SecureStore.getItemAsync(KEY);
  if (!raw) return null;

  try {
    return JSON.parse(raw) as DeviceConfig;
  } catch {
    return null;
  }
}

export async function saveDevice(device: DeviceConfig): Promise<void> {
  await SecureStore.setItemAsync(KEY, JSON.stringify(device));
}

export async function clearDevice(): Promise<void> {
  await SecureStore.deleteItemAsync(KEY);
}
