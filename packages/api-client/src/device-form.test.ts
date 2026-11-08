import { describe, expect, it } from "vitest";
import {
  deviceFormSchema,
  toDeviceConfig,
  type DeviceFormMessages,
} from "./device-form";

const messages: DeviceFormMessages = {
  hostRequired: "enter the device address",
  hostNoScheme: "drop the scheme, just the host or ip",
  portNumber: "port must be a number",
  portRange: "port must be between 1 and 65535",
  tokenRequired: "enter the api token",
};

const schema = deviceFormSchema(messages);

describe("deviceFormSchema", () => {
  it("accepts a host, port and token", () => {
    const parsed = schema.parse({
      host: "192.168.1.50",
      port: "8080",
      token: "abc",
    });

    expect(parsed).toEqual({ host: "192.168.1.50", port: 8080, token: "abc" });
  });

  it("defaults an empty port to 80", () => {
    expect(
      schema.parse({ host: "aquarium.local", port: "", token: "abc" }).port,
    ).toBe(80);
  });

  it("trims surrounding whitespace", () => {
    const parsed = schema.parse({
      host: "  aquarium.local  ",
      port: " 80 ",
      token: "  abc  ",
    });

    expect(parsed).toEqual({ host: "aquarium.local", port: 80, token: "abc" });
  });

  it("refuses a host carrying a scheme, with the supplied message", () => {
    const result = schema.safeParse({
      host: "http://192.168.1.50",
      port: "80",
      token: "abc",
    });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.issues[0]?.message).toBe(messages.hostNoScheme);
    }
  });

  it("refuses an empty host and an empty token", () => {
    expect(
      schema.safeParse({ host: "", port: "80", token: "abc" }).success,
    ).toBe(false);
    expect(schema.safeParse({ host: "a", port: "80", token: "" }).success).toBe(
      false,
    );
  });

  it("refuses a non-numeric port and one out of range", () => {
    expect(
      schema.safeParse({ host: "a", port: "http", token: "t" }).success,
    ).toBe(false);
    expect(schema.safeParse({ host: "a", port: "0", token: "t" }).success).toBe(
      false,
    );
    expect(
      schema.safeParse({ host: "a", port: "65536", token: "t" }).success,
    ).toBe(false);
  });
});

describe("toDeviceConfig", () => {
  it("produces a DeviceConfig", () => {
    expect(
      toDeviceConfig({ host: "aquarium.local", port: 80, token: "abc" }),
    ).toEqual({ host: "aquarium.local", port: 80, token: "abc" });
  });
});
