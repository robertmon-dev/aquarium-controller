import { ApiError, type DeviceConfig } from "@aquarium/api-client";
import { translator } from "@aquarium/i18n";
import { describe, expect, it } from "vitest";
import { describeProbeFailure } from "./probe-failure";

const device: DeviceConfig = {
  host: "192.168.1.25",
  port: 80,
  token: "t",
};
const t = translator("en");

describe("describeProbeFailure", () => {
  it("names a rejected token, which is not a reachability problem", () => {
    const message = describeProbeFailure(
      new ApiError("unauthorized", "nope", 401),
      device,
      t,
    );

    expect(message).toBe(t("deviceForm.tokenRejected"));
  });

  it("reports host and port when the device cannot be reached", () => {
    const message = describeProbeFailure(
      new ApiError("transport", "cannot reach"),
      device,
      t,
    );

    expect(message).toContain("192.168.1.25");
    expect(message).toContain("80");
  });

  it("falls back to unreachable for anything that is not an ApiError", () => {
    expect(describeProbeFailure(new Error("boom"), device, t)).toContain(
      "192.168.1.25",
    );
    expect(describeProbeFailure("a string", device, t)).toContain(
      "192.168.1.25",
    );
  });

  it("treats a non-auth ApiError as unreachable rather than guessing", () => {
    const message = describeProbeFailure(
      new ApiError("malformed", "bad json", 200),
      device,
      t,
    );

    expect(message).not.toBe(t("deviceForm.tokenRejected"));
  });
});
