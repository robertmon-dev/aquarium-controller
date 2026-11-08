import { describe, expect, it } from "vitest";
import {
  durationChoices,
  channelRequest,
  profileRequest,
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
  it("wraps channel levels in the externally tagged target", () => {
    expect(channelRequest({ white_pct: 20, rgb_pct: 10 }, 3600)).toEqual({
      target: { channels: { white_pct: 20, rgb_pct: 10 } },
      duration_s: 3600,
    });
  });

  it("clamps levels into the range the domain accepts", () => {
    expect(
      channelRequest({ white_pct: 140, rgb_pct: -5 }, null).target,
    ).toEqual({
      channels: { white_pct: 100, rgb_pct: 0 },
    });
  });

  it("rounds fractional slider values to integers", () => {
    expect(
      channelRequest({ white_pct: 59.6, rgb_pct: 40.2 }, null).target,
    ).toEqual({
      channels: { white_pct: 60, rgb_pct: 40 },
    });
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
