import { describe, expect, it } from "vitest";
import {
  durationChoices,
  channelRequest,
  profileRequest,
  sumpLightDurations,
  tickCountdown,
} from "./lighting-request";
import { translator } from "@aquarium/i18n";

const t = translator("en");

describe("profileRequest", () => {
  it("wraps a profile in the externally tagged target", () => {
    expect(profileRequest("moonlight", 300)).toEqual({
      target: { profile: "moonlight" },
      duration_s: 300,
    });
  });

  it("sends null for no limit, never zero", () => {
    expect(profileRequest("daylight", null)).toEqual({
      target: { profile: "daylight" },
      duration_s: null,
    });
  });
});

describe("channelRequest", () => {
  it("clamps and rounds every RGBW channel", () => {
    expect(
      channelRequest(
        { red_pct: -5, green_pct: 40.6, blue_pct: 120, white_pct: 60 },
        null,
      ),
    ).toEqual({
      target: {
        channels: { red_pct: 0, green_pct: 41, blue_pct: 100, white_pct: 60 },
      },
      duration_s: null,
    });
  });
});

describe("sumpLightDurations", () => {
  it("offers 30 min, 1 h and no limit", () => {
    const t = (key: string) => key;
    expect(sumpLightDurations(t as never).map((c) => c.seconds)).toEqual([
      1800,
      3600,
      null,
    ]);
  });
});

describe("durationChoices", () => {
  it("offers no limit as null rather than zero", () => {
    const noLimit = durationChoices(t).find((c) => c.label === "no limit");

    expect(noLimit?.seconds).toBeNull();
  });

  it("never offers a zero duration, which the firmware rejects", () => {
    expect(
      durationChoices(t).every((c) => c.seconds === null || c.seconds > 0),
    ).toBe(true);
  });
});

describe("tickCountdown", () => {
  it("counts a second down", () => {
    expect(tickCountdown(42)).toEqual({ next: 41, elapsed: false });
  });

  it("clamps at zero and reports elapsed", () => {
    expect(tickCountdown(1)).toEqual({ next: 0, elapsed: true });
  });

  it("stays at zero without reporting elapsed twice", () => {
    expect(tickCountdown(0)).toEqual({ next: 0, elapsed: false });
  });

  it("passes null through, since no override means no countdown", () => {
    expect(tickCountdown(null)).toEqual({ next: null, elapsed: false });
  });
});
