import { describe, expect, it } from "vitest";
import { initialProbe, probeReducer, type ProbeAction } from "./probe-reducer";

function run(...actions: ProbeAction[]) {
  return actions.reduce(probeReducer, initialProbe);
}

describe("probeReducer", () => {
  it("starts idle", () => {
    expect(initialProbe).toEqual({ status: "idle" });
  });

  it("remembers which action is probing", () => {
    expect(run({ type: "start", action: "test" })).toEqual({
      status: "probing",
      action: "test",
    });
    expect(run({ type: "start", action: "save" })).toEqual({
      status: "probing",
      action: "save",
    });
  });

  it("clears a previous failure when a new probe starts", () => {
    const state = run(
      { type: "failed", message: "could not reach" },
      { type: "start", action: "test" },
    );

    expect(state).toEqual({ status: "probing", action: "test" });
  });

  it("clears a previous success when a new probe starts", () => {
    const state = run({ type: "reachable" }, { type: "start", action: "save" });

    expect(state).toEqual({ status: "probing", action: "save" });
  });

  it("ends probing when the device answers", () => {
    const state = run({ type: "start", action: "test" }, { type: "reachable" });

    expect(state).toEqual({ status: "reachable" });
  });

  it("ends probing and keeps the reason when it fails", () => {
    const state = run(
      { type: "start", action: "save" },
      { type: "failed", message: "token rejected" },
    );

    expect(state).toEqual({ status: "failed", message: "token rejected" });
  });

  it("dismisses a failure back to idle", () => {
    const state = run({ type: "failed", message: "nope" }, { type: "dismiss" });

    expect(state).toEqual({ status: "idle" });
  });
});
