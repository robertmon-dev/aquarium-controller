export const RANGES = {
  h1: 3600,
  h6: 6 * 3600,
  h24: 24 * 3600,
  d7: 7 * 24 * 3600,
  d30: 30 * 24 * 3600,
} as const;

export type RangeKey = keyof typeof RANGES;

export type HistoryView = "chart" | "table";

export interface HistoryState {
  rangeKey: RangeKey;
  forceRaw: boolean;
  view: HistoryView;
  changedOnly: boolean;
}

export type HistoryAction =
  | { type: "setRange"; key: RangeKey }
  | { type: "forceRaw"; value: boolean }
  | { type: "setView"; view: HistoryView }
  | { type: "changedOnly"; value: boolean };

export const initialHistoryState: HistoryState = {
  rangeKey: "h24",
  forceRaw: false,
  view: "chart",
  changedOnly: false,
};

export function historyReducer(
  state: HistoryState,
  action: HistoryAction,
): HistoryState {
  switch (action.type) {
    case "setRange":
      if (!(action.key in RANGES)) return state;
      return { ...state, rangeKey: action.key, forceRaw: false };

    case "forceRaw":
      return { ...state, forceRaw: action.value };

    case "setView":
      return { ...state, view: action.view };

    case "changedOnly":
      return { ...state, changedOnly: action.value };
  }
}
