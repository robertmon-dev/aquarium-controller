import { describe, expect, it } from "vitest";
import {
  historyReducer,
  initialHistoryState,
  type HistoryAction,
} from "./history-reducer";

function run(...actions: HistoryAction[]) {
  return actions.reduce(historyReducer, initialHistoryState);
}

describe("historyReducer", () => {
  it("starts on the last 24 hours, charting, with every chart shown", () => {
    expect(initialHistoryState).toMatchObject({
      rangeKey: "h24",
      forceRaw: false,
      view: "chart",
      changedOnly: false,
    });
  });

  it("switching range clears a forced raw request", () => {
    const state = run(
      { type: "forceRaw", value: true },
      { type: "setRange", key: "d7" },
    );

    expect(state.rangeKey).toBe("d7");
    expect(state.forceRaw).toBe(false);
  });

  it("keeps raw as a deliberate choice once the range is set", () => {
    const state = run(
      { type: "setRange", key: "d7" },
      { type: "forceRaw", value: true },
    );

    expect(state).toMatchObject({ rangeKey: "d7", forceRaw: true });
  });

  it("toggles between chart and table", () => {
    expect(run({ type: "setView", view: "table" }).view).toBe("table");
    expect(
      run(
        { type: "setView", view: "table" },
        { type: "setView", view: "chart" },
      ).view,
    ).toBe("chart");
  });

  it("toggling changed-only does not disturb the range", () => {
    const state = run(
      { type: "setRange", key: "h6" },
      { type: "changedOnly", value: true },
    );

    expect(state).toMatchObject({ rangeKey: "h6", changedOnly: true });
  });

  it("refuses an unknown range key by keeping the current one", () => {
    const state = run({ type: "setRange", key: "nonsense" as never });

    expect(state.rangeKey).toBe(initialHistoryState.rangeKey);
  });
});
