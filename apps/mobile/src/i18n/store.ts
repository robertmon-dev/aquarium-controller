import * as SecureStore from "expo-secure-store";
import { isLocale, type Locale } from ".";

const KEY = "aquarium.locale";

export async function loadLocale(): Promise<Locale | null> {
  const raw = await SecureStore.getItemAsync(KEY);
  return isLocale(raw) ? raw : null;
}

export async function saveLocale(locale: Locale): Promise<void> {
  await SecureStore.setItemAsync(KEY, locale);
}
