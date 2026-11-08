import {
  LocaleProvider as BaseLocaleProvider,
  useLocale,
  useT,
} from "@aquarium/i18n/use-locale";
import type { ReactNode } from "react";
import { expoLocaleAdapter } from "./expo-adapter";

export { useLocale, useT };

export function LocaleProvider({ children }: { children: ReactNode }) {
  return (
    <BaseLocaleProvider adapter={expoLocaleAdapter}>
      {children}
    </BaseLocaleProvider>
  );
}
