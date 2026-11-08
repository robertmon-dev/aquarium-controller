import { describe, expect, it } from "vitest";
import { translator } from "../i18n";
import { deviceFormMessages } from "./config";

describe("deviceFormMessages", () => {
  it("resolves every message the schema needs", () => {
    const messages = deviceFormMessages(translator("en"));

    expect(Object.keys(messages).sort()).toEqual([
      "hostNoScheme",
      "hostRequired",
      "portNumber",
      "portRange",
      "tokenRequired",
    ]);
    for (const value of Object.values(messages)) {
      expect(value.trim()).not.toBe("");
    }
  });

  it("resolves no missing-translation placeholders", () => {
    for (const locale of ["en", "pl"] as const) {
      for (const value of Object.values(
        deviceFormMessages(translator(locale)),
      )) {
        expect(value).not.toContain("missing");
      }
    }
  });

  it("translates, so the form speaks the chosen language", () => {
    expect(deviceFormMessages(translator("pl")).hostRequired).not.toBe(
      deviceFormMessages(translator("en")).hostRequired,
    );
  });
});
