import { describe, expect, it } from "vitest";
import { deviceFormSchema, toDeviceConfig } from "./config";
import { translator } from "../i18n";

const schema = deviceFormSchema(translator("en"));

describe("deviceFormSchema", () => {
  it("accepts a host, port and token", () => {
    const parsed = schema.parse({
      host: "192.168.1.50",
      port: "80",
      token: "abc123",
    });

    expect(parsed.port).toBe(80);
  });

  it("defaults the port to 80 when left empty", () => {
    expect(schema.parse({ host: "10.0.0.2", port: "", token: "t" }).port).toBe(
      80,
    );
  });

  it("rejects an empty host", () => {
    expect(() => schema.parse({ host: "", port: "80", token: "t" })).toThrow();
  });

  it("rejects a host with a scheme, since the client adds it", () => {
    expect(() =>
      schema.parse({
        host: "http://10.0.0.2",
        port: "80",
        token: "t",
      }),
    ).toThrow(/scheme/);
  });

  it("rejects a non-numeric port", () => {
    expect(() =>
      schema.parse({ host: "10.0.0.2", port: "80a", token: "t" }),
    ).toThrow();
  });

  it("rejects a port outside the valid range", () => {
    expect(() =>
      schema.parse({ host: "10.0.0.2", port: "70000", token: "t" }),
    ).toThrow();
  });

  it("rejects an empty token", () => {
    expect(() =>
      schema.parse({ host: "10.0.0.2", port: "80", token: "" }),
    ).toThrow();
  });
});

describe("toDeviceConfig", () => {
  it("trims whitespace pasted around the token", () => {
    const config = toDeviceConfig({
      host: " 10.0.0.2 ",
      port: 80,
      token: " tok\n",
    });

    expect(config).toEqual({ host: "10.0.0.2", port: 80, token: "tok" });
  });
});
