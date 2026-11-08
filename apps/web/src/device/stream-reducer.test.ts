import { describe, expect, it } from "vitest";
import {
  initialStreamState,
  streamReducer,
  type StreamAction,
} from "./stream-reducer";

function run(...actions: StreamAction[]) {
  return actions.reduce(streamReducer, initialStreamState);
}

describe("streamReducer", () => {
  it("starts idle with no retries and no event seen", () => {
    expect(initialStreamState).toEqual({
      status: "idle",
      retries: 0,
      lastEventAt: null,
    });
  });

  it("goes connecting when the stream opens", () => {
    expect(run({ type: "connecting" }).status).toBe("connecting");
  });

  it("goes live on the first snapshot and records when it arrived", () => {
    const state = run({ type: "connecting" }, { type: "snapshot", at: 1000 });

    expect(state.status).toBe("live");
    expect(state.lastEventAt).toBe(1000);
  });

  it("clears the retry count once live", () => {
    const state = run(
      { type: "connecting" },
      { type: "dropped" },
      { type: "dropped" },
      { type: "snapshot", at: 50 },
    );

    expect(state).toEqual({ status: "live", retries: 0, lastEventAt: 50 });
  });

  it("counts retries while reconnecting", () => {
    const state = run({ type: "dropped" }, { type: "dropped" });

    expect(state).toMatchObject({ status: "reconnecting", retries: 2 });
  });

  it("keeps the last event time across a drop, so the ui can age it", () => {
    const state = run({ type: "snapshot", at: 900 }, { type: "dropped" });

    expect(state).toMatchObject({ status: "reconnecting", lastEventAt: 900 });
  });

  it("is terminal once unauthorized", () => {
    const state = run(
      { type: "unauthorized" },
      { type: "dropped" },
      { type: "snapshot", at: 10 },
    );

    expect(state.status).toBe("unauthorized");
  });

  it("resets completely when the device changes", () => {
    const state = run(
      { type: "snapshot", at: 10 },
      { type: "dropped" },
      { type: "reset" },
    );

    expect(state).toEqual(initialStreamState);
  });
});
