import { getLocales } from "expo-localization";

export function deviceLanguageCode(): string | undefined {
  return getLocales()[0]?.languageCode ?? undefined;
}
