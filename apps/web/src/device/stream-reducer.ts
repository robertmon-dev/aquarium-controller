import type { StreamState } from "@aquarium/view-model";

export interface StreamStatus {
  status: StreamState;
  retries: number;
  lastEventAt: number | null;
}

export type StreamAction =
  | { type: "connecting" }
  | { type: "snapshot"; at: number }
  | { type: "dropped" }
  | { type: "unauthorized" }
  | { type: "reset" };

export const initialStreamState: StreamStatus = {
  status: "idle",
  retries: 0,
  lastEventAt: null,
};

export function streamReducer(
  state: StreamStatus,
  action: StreamAction,
): StreamStatus {
  if (action.type === "reset") return initialStreamState;

  if (state.status === "unauthorized") return state;

  switch (action.type) {
    case "connecting":
      return { ...state, status: "connecting" };

    case "snapshot":
      return { status: "live", retries: 0, lastEventAt: action.at };

    case "dropped":
      return { ...state, status: "reconnecting", retries: state.retries + 1 };

    case "unauthorized":
      return { ...state, status: "unauthorized" };
  }
}
