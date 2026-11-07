import { describe, expect, it } from "vitest";
import { FALLBACK_LOCALE, resolveLocale, translator } from ".";
import { en } from "./locales/en";
import { pl } from "./locales/pl";

function keyPaths(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) return [prefix];

  return Object.entries(value).flatMap(([key, nested]) =>
    keyPaths(nested, prefix ? `${prefix}.${key}` : key),
  );
}

describe("the locale catalogues", () => {
  it("carry exactly the same keys", () => {
    expect(keyPaths(pl).sort()).toEqual(keyPaths(en).sort());
  });

  it("leave no value empty, which would render as a blank label", () => {
    const blanks = keyPaths(pl).filter(
      (key) => translator("pl")(key as never).trim() === "",
    );

    expect(blanks).toEqual([]);
  });

  it("actually differ, so the switcher has something to switch", () => {
    expect(translator("pl")("lighting.restore")).not.toBe(
      translator("en")("lighting.restore"),
    );
  });

  it("interpolates into both languages", () => {
    for (const locale of ["en", "pl"] as const) {
      expect(
        translator(locale)("deviceForm.unreachable", {
          host: "192.168.1.25",
          port: 80,
        }),
      ).toContain("192.168.1.25");
    }
  });
});

describe("resolveLocale", () => {
  it("takes the system language when nothing is stored", () => {
    expect(resolveLocale(null, "pl")).toBe("pl");
  });

  it("falls back to english for a language we do not carry", () => {
    expect(resolveLocale(null, "de")).toBe(FALLBACK_LOCALE);
    expect(resolveLocale(null, undefined)).toBe(FALLBACK_LOCALE);
  });

  it("lets a stored choice beat the system language", () => {
    expect(resolveLocale("en", "pl")).toBe("en");
    expect(resolveLocale("pl", "en")).toBe("pl");
  });

  it("ignores a stored value that is not a language we carry", () => {
    expect(resolveLocale("klingon", "pl")).toBe("pl");
  });
});
