import { describe, expect, it } from "vitest";
import {
  initialLightingState,
  lightingReducer,
  type LightingAction,
} from "./lighting-reducer";

const start = initialLightingState({
  red_pct: 10,
  green_pct: 10,
  blue_pct: 20,
  white_pct: 40,
});

function run(...actions: LightingAction[]) {
  return actions.reduce(lightingReducer, start);
}

describe("lightingReducer", () => {
  it("starts on schedule, with the device's current levels and no dirt", () => {
    expect(start).toEqual({
      mode: "schedule",
      levels: { red_pct: 10, green_pct: 10, blue_pct: 20, white_pct: 40 },
      duration: 900,
      dirty: false,
      confirming: false,
    });
  });

  it("picking a profile switches mode and marks dirty", () => {
    const state = run({ type: "pickProfile", profile: "daylight" });

    expect(state).toMatchObject({
      mode: "profile",
      profile: "daylight",
      dirty: true,
    });
  });

  it("moving a channel switches to channel mode and keeps both levels", () => {
    const state = run({ type: "setChannel", channel: "white_pct", value: 80 });

    expect(state.mode).toBe("channels");
    expect(state.levels).toEqual({
      red_pct: 10,
      green_pct: 10,
      blue_pct: 20,
      white_pct: 80,
    });
    expect(state.dirty).toBe(true);
  });

  it("clamps a channel to 0..100", () => {
    expect(
      run({ type: "setChannel", channel: "blue_pct", value: 140 }).levels
        .blue_pct,
    ).toBe(100);
    expect(
      run({ type: "setChannel", channel: "blue_pct", value: -5 }).levels
        .blue_pct,
    ).toBe(0);
  });

  it("picking a profile after dragging drops the channel mode", () => {
    const state = run(
      { type: "setChannel", channel: "white_pct", value: 80 },
      { type: "pickProfile", profile: "moonlight" },
    );

    expect(state.mode).toBe("profile");
    expect(state.profile).toBe("moonlight");
  });

  it("changing the duration does not make the form dirty on its own", () => {
    const state = run({ type: "setDuration", seconds: 3600 });

    expect(state.duration).toBe(3600);
    expect(state.dirty).toBe(false);
  });

  it("accepts an unlimited duration", () => {
    expect(run({ type: "setDuration", seconds: null }).duration).toBeNull();
  });

  it("restoring the schedule resets mode, levels and dirt", () => {
    const state = run(
      { type: "setChannel", channel: "white_pct", value: 90 },
      {
        type: "restore",
        levels: { red_pct: 15, green_pct: 15, blue_pct: 35, white_pct: 55 },
      },
    );

    expect(state).toMatchObject({
      mode: "schedule",
      levels: { red_pct: 15, green_pct: 15, blue_pct: 35, white_pct: 55 },
      dirty: false,
    });
  });

  it("a committed override is no longer dirty", () => {
    const state = run(
      { type: "pickProfile", profile: "sunset" },
      { type: "committed" },
    );

    expect(state.dirty).toBe(false);
    expect(state.mode).toBe("profile");
  });

  it("opens and closes the restore confirmation", () => {
    expect(run({ type: "confirmRestore" }).confirming).toBe(true);
    expect(
      run({ type: "confirmRestore" }, { type: "cancelRestore" }).confirming,
    ).toBe(false);
  });

  it("closes the confirmation as part of restoring, not as a second step", () => {
    const state = run(
      { type: "setChannel", channel: "white_pct", value: 90 },
      { type: "confirmRestore" },
      {
        type: "restore",
        levels: { red_pct: 15, green_pct: 15, blue_pct: 35, white_pct: 55 },
      },
    );

    expect(state).toMatchObject({
      mode: "schedule",
      levels: { red_pct: 15, green_pct: 15, blue_pct: 35, white_pct: 55 },
      dirty: false,
      confirming: false,
    });
  });

  it("leaves the confirmation alone when an override commits", () => {
    const state = run(
      { type: "confirmRestore" },
      { type: "pickProfile", profile: "sunset" },
      { type: "committed" },
    );

    expect(state.confirming).toBe(true);
  });
});
