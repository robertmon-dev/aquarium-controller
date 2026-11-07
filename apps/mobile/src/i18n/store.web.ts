import { isLocale, type Locale } from ".";

const KEY = "aquarium.locale";

export async function loadLocale(): Promise<Locale | null> {
  const raw = localStorage.getItem(KEY);
  return isLocale(raw) ? raw : null;
}

export async function saveLocale(locale: Locale): Promise<void> {
  localStorage.setItem(KEY, locale);
}
