import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { parseAnnotatedCsv, type HistorySeries } from "./csv";
import { chartFluxRequest, fluxQuery, type HistoryWindow } from "./flux";
import type { ChartSpec } from "./series";

export type { HistoryWindow } from "./flux";

const ORG = "home";
const PROXY = "/influx/api/v2/query";

async function fetchSeries(
  chart: ChartSpec,
  window: HistoryWindow,
  forceRaw: boolean,
): Promise<HistorySeries[]> {
  const query = fluxQuery(chartFluxRequest(chart, window, forceRaw));

  const response = await fetch(`${PROXY}?org=${encodeURIComponent(ORG)}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/csv",
    },
    body: JSON.stringify({
      query,
      dialect: { annotations: ["datatype", "group", "default"] },
    }),
  });

  if (!response.ok) {
    throw new Error(`influx returned ${response.status}`);
  }

  return parseAnnotatedCsv(await response.text());
}

export function useHistory(
  chart: ChartSpec,
  window: HistoryWindow,
  forceRaw: boolean,
): UseQueryResult<HistorySeries[]> {
  return useQuery({
    queryKey: [
      "history",
      chart.id,
      window.start,
      window.stop,
      forceRaw,
    ] as const,
    queryFn: () => fetchSeries(chart, window, forceRaw),
    staleTime: 30_000,
  });
}
