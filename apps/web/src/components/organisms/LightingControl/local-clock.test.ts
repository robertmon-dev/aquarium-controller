import { describe, expect, it } from "vitest";
import { localClockFrom } from "./local-clock";

describe("localClockFrom", () => {
  it("shifts the month into the 1-12 the firmware uses", () => {
    expect(localClockFrom(new Date(2026, 0, 1)).month).toBe(1);
    expect(localClockFrom(new Date(2026, 11, 31)).month).toBe(12);
  });

  it("counts minutes from midnight", () => {
    expect(localClockFrom(new Date(2026, 5, 1, 0, 0)).minuteOfDay).toBe(0);
    expect(localClockFrom(new Date(2026, 5, 1, 18, 30)).minuteOfDay).toBe(1110);
    expect(localClockFrom(new Date(2026, 5, 1, 23, 59)).minuteOfDay).toBe(1439);
  });

  it("never reaches a full day, so the marker stays inside the axis", () => {
    const latest = localClockFrom(new Date(2026, 5, 1, 23, 59));

    expect(latest.minuteOfDay).toBeLessThan(1440);
  });
});
