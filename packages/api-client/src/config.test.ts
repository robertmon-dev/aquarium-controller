import { describe, expect, it } from "vitest";
import { baseUrl, endpointUrl, eventsUrl } from "./config";

const device = { host: "192.168.1.50", port: 80, token: "s3cret" };

describe("baseUrl", () => {
  it("builds an http origin from host and port", () => {
    expect(baseUrl(device)).toBe("http://192.168.1.50:80");
  });
});

describe("endpointUrl", () => {
  it("joins the origin with an api path", () => {
    expect(endpointUrl(device, "/api/telemetry")).toBe(
      "http://192.168.1.50:80/api/telemetry",
    );
  });
});

describe("eventsUrl", () => {
  it("carries the token in the query string", () => {
    expect(eventsUrl(device)).toBe(
      "http://192.168.1.50:80/api/events?token=s3cret",
    );
  });

  it("percent-encodes a token containing url-unsafe characters", () => {
    expect(eventsUrl({ ...device, token: "a+b/c=" })).toBe(
      "http://192.168.1.50:80/api/events?token=a%2Bb%2Fc%3D",
    );
  });
});
