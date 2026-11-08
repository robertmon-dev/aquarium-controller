import { useMemo } from "react";
import type { HistoryWindow } from "@/history/queries";
import { RANGES, type RangeKey } from "./history-reducer";

/**
 * The window is recomputed only when the range changes, so a re-render does not
 * silently shift `stop` and invalidate every chart's query key.
 */
export function useHistoryWindow(rangeKey: RangeKey): HistoryWindow {
  return useMemo(() => {
    const seconds = RANGES[rangeKey];
    const stop = new Date();
    const start = new Date(stop.getTime() - seconds * 1000);

    return {
      start: start.toISOString(),
      stop: stop.toISOString(),
      seconds,
    };
  }, [rangeKey]);
}
