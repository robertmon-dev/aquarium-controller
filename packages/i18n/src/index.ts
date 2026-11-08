import { I18n } from "i18n-js";
import { en } from "./locales/en";
import { pl } from "./locales/pl";
import type { Leaves } from "./types";

export const LOCALES = ["en", "pl"] as const;

export type Locale = (typeof LOCALES)[number];

export const FALLBACK_LOCALE: Locale = "en";

export type TranslationKey = Leaves<typeof en>;

export type Translate = (
  key: TranslationKey,
  options?: Record<string, unknown>,
) => string;

const i18n = new I18n({ en, pl });
i18n.defaultLocale = FALLBACK_LOCALE;
i18n.enableFallback = true;

export function isLocale(value: string | null | undefined): value is Locale {
  return LOCALES.includes(value as Locale);
}

export function resolveLocale(
  stored: string | null | undefined,
  system: string | null | undefined,
): Locale {
  if (isLocale(stored)) return stored;
  return isLocale(system) ? system : FALLBACK_LOCALE;
}

export function translator(locale: Locale): Translate {
  return (key, options) => i18n.t(key, { locale, ...options });
}
