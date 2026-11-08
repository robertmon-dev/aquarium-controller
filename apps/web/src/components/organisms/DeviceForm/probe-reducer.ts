export type ProbeTrigger = "test" | "save";

export type ProbeState =
  | { status: "idle" }
  | { status: "probing"; action: ProbeTrigger }
  | { status: "reachable" }
  | { status: "failed"; message: string };

export type ProbeAction =
  | { type: "start"; action: ProbeTrigger }
  | { type: "reachable" }
  | { type: "failed"; message: string }
  | { type: "dismiss" };

export const initialProbe: ProbeState = { status: "idle" };

export function probeReducer(
  _state: ProbeState,
  action: ProbeAction,
): ProbeState {
  switch (action.type) {
    case "start":
      return { status: "probing", action: action.action };

    case "reachable":
      return { status: "reachable" };

    case "failed":
      return { status: "failed", message: action.message };

    case "dismiss":
      return { status: "idle" };
  }
}
