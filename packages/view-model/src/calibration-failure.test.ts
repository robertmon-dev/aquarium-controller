import { ApiError } from "@aquarium/api-client";
import { translator } from "@aquarium/i18n";
import { describe, expect, it } from "vitest";
import { describeCalibrationFailure } from "./calibration-failure";

const t = translator("en");

describe("describeCalibrationFailure", () => {
  it("says the calibration would not survive a reboot when the device cannot store it", () => {
    const error = new ApiError("status", "device returned 500", 500, "nvs");

    expect(describeCalibrationFailure(error, t)).toBe(
      t("calibration.notStored"),
    );
  });

  it("repeats the reason the device gave for rejecting the values", () => {
    const error = new ApiError(
      "status",
      "device returned 400",
      400,
      "TDS factor must be between 0.1 and 10.0",
    );

    expect(describeCalibrationFailure(error, t)).toBe(
      "TDS factor must be between 0.1 and 10.0",
    );
  });

  it("names a rejected token rather than blaming the values", () => {
    const error = new ApiError("unauthorized", "nope", 401);

    expect(describeCalibrationFailure(error, t)).toBe(
      t("deviceForm.tokenRejected"),
    );
  });

  it("falls back to the error message when the device sent no reason", () => {
    const error = new ApiError("status", "device returned 400", 400, "   ");

    expect(describeCalibrationFailure(error, t)).toBe("device returned 400");
  });

  it("falls back to a generic message for anything that is not an api error", () => {
    expect(describeCalibrationFailure(new Error("boom"), t)).toBe(
      t("calibration.saveFailed"),
    );
  });
});
