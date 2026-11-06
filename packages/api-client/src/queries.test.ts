import { describe, expect, it } from "vitest";
import { queryKeys } from "./query-keys";

const a = { host: "10.0.0.2", port: 80, token: "tok" };
const b = { host: "10.0.0.3", port: 80, token: "tok" };

describe("queryKeys", () => {
  it("is stable across calls for the same device", () => {
    expect(queryKeys.telemetry(a)).toEqual(queryKeys.telemetry(a));
  });

  it("separates devices on different hosts", () => {
    expect(queryKeys.telemetry(a)).not.toEqual(queryKeys.telemetry(b));
  });

  it("separates devices on different ports", () => {
    expect(queryKeys.telemetry(a)).not.toEqual(
      queryKeys.telemetry({ ...a, port: 8080 }),
    );
  });

  it("does not put the token in the cache key", () => {
    expect(JSON.stringify(queryKeys.telemetry(a))).not.toContain("tok");
  });
});
