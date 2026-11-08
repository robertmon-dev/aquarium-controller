import { isLocale, type Locale } from ".";

export interface LocaleAdapter {
  load(): Promise<Locale | null>;
  save(locale: Locale): Promise<void>;
  systemLanguage(): string | undefined;
}

export interface MemoryAdapter extends LocaleAdapter {
  saved: Locale[];
}

export function memoryAdapter(options: {
  stored: string | null;
  system: string | undefined;
}): MemoryAdapter {
  const saved: Locale[] = [];

  return {
    saved,
    load: () =>
      Promise.resolve(isLocale(options.stored) ? options.stored : null),
    save: (locale) => {
      saved.push(locale);
      return Promise.resolve();
    },
    systemLanguage: () => options.system,
  };
}

export { isLocale } from ".";
export type { Locale } from ".";
