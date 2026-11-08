import { ApiError } from "@aquarium/api-client";
import { describe, expect, it } from "vitest";
import { errorCodeFor } from "./error-code";

describe("errorCodeFor", () => {
  it("separates a rejected token from an unreachable device", () => {
    expect(errorCodeFor(new ApiError("unauthorized", "no", 401))).toBe(
      "unauthorized",
    );
    expect(errorCodeFor(new ApiError("transport", "no"))).toBe("offline");
  });

  it("treats a bad body or a bad status as the device misbehaving", () => {
    expect(errorCodeFor(new ApiError("malformed", "no", 200))).toBe(
      "device-error",
    );
    expect(errorCodeFor(new ApiError("status", "no", 503))).toBe(
      "device-error",
    );
  });

  it("does not guess for anything that is not an ApiError", () => {
    expect(errorCodeFor(new Error("boom"))).toBe("device-error");
    expect(errorCodeFor(undefined)).toBe("device-error");
  });
});
