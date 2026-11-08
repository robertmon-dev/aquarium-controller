import {
  isLocale,
  type Locale,
  type LocaleAdapter,
} from "@aquarium/i18n/adapter";

const KEY = "aquarium.locale";

export const webLocaleAdapter: LocaleAdapter = {
  load(): Promise<Locale | null> {
    const raw = localStorage.getItem(KEY);
    return Promise.resolve(isLocale(raw) ? raw : null);
  },
  save(locale: Locale): Promise<void> {
    localStorage.setItem(KEY, locale);
    return Promise.resolve();
  },
  systemLanguage: () => navigator.language.split("-")[0],
};
