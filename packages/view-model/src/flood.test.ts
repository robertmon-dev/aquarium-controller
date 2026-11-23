import { translator } from "@aquarium/i18n";
import { describe, expect, it } from "vitest";
import { canResetFlood, floodMessage } from "./flood";

const t = translator("pl");

describe("flood alarm", () => {
  it.each([
    [true, true, t("telemetry.floodBoth")],
    [true, false, t("telemetry.floodReservoir")],
    [false, true, t("telemetry.floodSump")],
    [false, false, t("telemetry.floodDry")],
  ] as const)("identifies reservoir=%s and sump=%s", (reservoir, sump, expected) => {
    const snapshot = {
      flood: { latched: true, probes: { reservoir, sump } },
      leak_detected: reservoir || sump,
    };
    expect(floodMessage(snapshot, t)).toBe(expected);
    expect(canResetFlood(snapshot)).toBe(!reservoir && !sump);
  });

  it("keeps an alarm visible after restart and supports older firmware", () => {
    const snapshot = { flood: { latched: true, probes: null }, leak_detected: false };
    expect(floodMessage(snapshot, t)).toBe(t("telemetry.floodUnknown"));
    expect(canResetFlood(snapshot)).toBe(false);
    expect(floodMessage({ leak_detected: true }, t)).toBe(t("telemetry.leakBody"));
    expect(floodMessage({ leak_detected: false }, t)).toBeNull();
  });
});
