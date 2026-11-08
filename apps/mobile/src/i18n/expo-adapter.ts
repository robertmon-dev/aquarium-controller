import {
  isLocale,
  type Locale,
  type LocaleAdapter,
} from "@aquarium/i18n/adapter";
import { getLocales } from "expo-localization";
import * as SecureStore from "expo-secure-store";

const KEY = "aquarium.locale";

export const expoLocaleAdapter: LocaleAdapter = {
  async load(): Promise<Locale | null> {
    const raw = await SecureStore.getItemAsync(KEY);
    return isLocale(raw) ? raw : null;
  },
  async save(locale: Locale): Promise<void> {
    await SecureStore.setItemAsync(KEY, locale);
  },
  systemLanguage: () => getLocales()[0]?.languageCode ?? undefined,
};
