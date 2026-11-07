import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  FALLBACK_LOCALE,
  resolveLocale,
  translator,
  type Locale,
  type Translate,
} from ".";
import { deviceLanguageCode } from "./system-locale";
import { loadLocale, saveLocale } from "./store";

interface LocaleContextValue {
  locale: Locale;
  setLocale: (locale: Locale) => Promise<void>;
  t: Translate;
}

const LocaleContext = createContext<LocaleContextValue | null>(null);

export function LocaleProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale | null>(null);

  useEffect(() => {
    loadLocale()
      .then((stored) =>
        setLocaleState(resolveLocale(stored, deviceLanguageCode())),
      )
      .catch(() => setLocaleState(resolveLocale(null, deviceLanguageCode())));
  }, []);

  const value = useMemo<LocaleContextValue>(() => {
    const active = locale ?? FALLBACK_LOCALE;

    return {
      locale: active,
      t: translator(active),
      setLocale: async (next) => {
        await saveLocale(next);
        setLocaleState(next);
      },
    };
  }, [locale]);

  return (
    <LocaleContext.Provider value={value}>{children}</LocaleContext.Provider>
  );
}

export function useLocale(): LocaleContextValue {
  const value = useContext(LocaleContext);
  if (!value) throw new Error("useLocale must be used inside LocaleProvider");
  return value;
}

export function useT(): Translate {
  return useLocale().t;
}
