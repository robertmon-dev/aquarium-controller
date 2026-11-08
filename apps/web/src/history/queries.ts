import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { parseAnnotatedCsv, type HistorySeries } from "./csv";
import { fluxQuery } from "./flux";
import { resolutionFor } from "./resolution";
import type { ChartSpec } from "./series";

const BUCKET = "aquarium";
const ORG = "home";
const PROXY = "/influx/api/v2/query";

export interface HistoryWindow {
  start: string;
  stop: string;
  seconds: number;
}

async function fetchSeries(
  chart: ChartSpec,
  window: HistoryWindow,
  forceRaw: boolean,
): Promise<HistorySeries[]> {
  const fields = chart.reference
    ? [...chart.fields, chart.reference]
    : chart.fields;

  const query = fluxQuery({
    bucket: BUCKET,
    measurement: chart.measurement,
    fields,
    start: window.start,
    stop: window.stop,
    resolution: resolutionFor(window.seconds, forceRaw),
    kind: chart.kind,
  });

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
