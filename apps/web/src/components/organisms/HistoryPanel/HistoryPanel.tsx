import { useT } from "@aquarium/i18n/use-locale";
import { useReducer } from "react";
import { Alert, Button } from "@/components/atoms";
import { LineChart } from "@/components/atoms/Chart/LineChart";
import { StateStrip } from "@/components/atoms/Chart/StateStrip";
import { ScreenMessage } from "@/components/molecules";
import { TimeRangePicker } from "@/components/molecules/TimeRangePicker/TimeRangePicker";
import { useHistory, type HistoryWindow } from "@/history/queries";
import { useHistoryWindow } from "./use-history-window";
import { CHARTS, type ChartSpec } from "@/history/series";
import { HistoryTable } from "./HistoryTable";
import { historyReducer, initialHistoryState } from "./history-reducer";
import "./HistoryPanel.scss";

export function HistoryPanel() {
  const t = useT();
  const [state, dispatch] = useReducer(historyReducer, initialHistoryState);

  const window = useHistoryWindow(state.rangeKey);

  return (
    <>
      <div className="history-controls">
        <TimeRangePicker
          selected={state.rangeKey}
          onSelect={(key) => dispatch({ type: "setRange", key })}
        />

        <Button
          variant={state.forceRaw ? "primary" : "outline"}
          onClick={() => dispatch({ type: "forceRaw", value: !state.forceRaw })}
        >
          {state.forceRaw ? t("history.raw") : t("history.aggregated")}
        </Button>

        <Button
          variant="outline"
          onClick={() =>
            dispatch({
              type: "setView",
              view: state.view === "chart" ? "table" : "chart",
            })
          }
        >
          {state.view === "chart" ? t("history.table") : t("history.chart")}
        </Button>
      </div>

      {CHARTS.map((chart) => (
        <HistoryChart
          key={chart.id}
          chart={chart}
          window={window}
          forceRaw={state.forceRaw}
          asTable={state.view === "table"}
        />
      ))}
    </>
  );
}

function HistoryChart({
  chart,
  window,
  forceRaw,
  asTable,
}: {
  chart: ChartSpec;
  window: HistoryWindow;
  forceRaw: boolean;
  asTable: boolean;
}) {
  const t = useT();
  const { data, error, isPending } = useHistory(chart, window, forceRaw);

  if (isPending)
    return <ScreenMessage loading>{t("history.loading")}</ScreenMessage>;

  if (error)
    return (
      <Alert variant="warning" title={t("history.failed")}>
        {error.message}
      </Alert>
    );

  if (!data || data.length === 0)
    return <ScreenMessage>{t("history.empty")}</ScreenMessage>;

  if (asTable) return <HistoryTable series={data} />;

  return chart.kind === "strip" ? (
    <StateStrip title={chart.id} series={data} />
  ) : (
    <LineChart
      title={chart.id}
      unit={chart.unit}
      series={data}
      {...(chart.reference ? { referenceField: chart.reference } : {})}
    />
  );
}
