import { useT } from "@aquarium/i18n/use-locale";
import type { Translate, TranslationKey } from "@aquarium/i18n";
import { Check } from "lucide-react";
import { useReducer } from "react";
import { Alert, Button, SegmentedControl } from "@/components/atoms";
import { LineChart } from "@/components/atoms/Chart/LineChart";
import { plotSeries, latestValue } from "@/components/atoms/Chart/chart-series";
import { StateStrip } from "@/components/atoms/Chart/StateStrip";
import { ScreenMessage } from "@/components/molecules";
import type { HistorySeries } from "@/history/csv";
import { useHistory, type HistoryWindow } from "@/history/queries";
import { CHARTS, type ChartSpec } from "@/history/series";
import { HistoryCard } from "./HistoryCard";
import { HistoryTable } from "./HistoryTable";
import {
  historyReducer,
  initialHistoryState,
  RANGES,
  type HistoryView,
  type RangeKey,
} from "./history-reducer";
import { formatSample } from "./readings";
import { useHistoryWindow } from "./use-history-window";
import "./HistoryPanel.scss";

const RANGE_KEYS = Object.keys(RANGES) as RangeKey[];
const VIEWS: HistoryView[] = ["chart", "table"];

/** The catalogue carries a label for every chart id and field in the registry. */
function label(t: Translate, path: string): string {
  return t(path as TranslationKey);
}

export function HistoryPanel() {
  const t = useT();
  const [state, dispatch] = useReducer(historyReducer, initialHistoryState);
  const window = useHistoryWindow(state.rangeKey);
  const averaged = !state.forceRaw;

  return (
    <>
      <div className="history-controls">
        <SegmentedControl
          ariaLabel={t("history.title")}
          selected={state.rangeKey}
          onSelect={(key) => dispatch({ type: "setRange", key })}
          segments={RANGE_KEYS.map((key) => ({
            value: key,
            label: label(t, `history.ranges.${key}`),
          }))}
        />

        <div className="history-controls__view">
          <Button
            variant={averaged ? "primary" : "outline"}
            aria-pressed={averaged}
            leftIcon={averaged ? <Check size={14} /> : undefined}
            onClick={() => dispatch({ type: "forceRaw", value: averaged })}
          >
            {t("history.averaging")}
          </Button>

          <SegmentedControl
            ariaLabel={t("history.chart")}
            selected={state.view}
            onSelect={(view) => dispatch({ type: "setView", view })}
            segments={VIEWS.map((view) => ({
              value: view,
              label: label(t, `history.${view}`),
            }))}
          />
        </div>
      </div>

      <div className="history-grid">
        {CHARTS.map((chart) => (
          <HistoryChart
            key={chart.id}
            chart={chart}
            window={window}
            forceRaw={state.forceRaw}
            asTable={state.view === "table"}
          />
        ))}
      </div>
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

  const fields = chart.reference
    ? [...chart.fields, chart.reference]
    : chart.fields;

  const plotted = plotSeries(fields, chart.reference);
  const series = data ?? [];

  const legend = plotted.map((one) => ({
    label: label(t, `history.fields.${one.field}`),
    color: one.color,
    isReference: one.isReference,
  }));

  const readings = chart.fields.map((field) => {
    const found = series.find((one) => one.field === field);

    return {
      label: label(t, `history.fields.${field}`),
      value: formatSample(
        found ? latestValue(found) : null,
        found?.kind ?? "number",
        chart.unit,
      ),
    };
  });

  const range: [number, number] = [
    Math.floor(Date.parse(window.start) / 1000),
    Math.floor(Date.parse(window.stop) / 1000),
  ];

  return (
    <HistoryCard
      title={label(t, `history.charts.${chart.id}`)}
      readings={readings}
      legend={legend}
    >
      <HistoryBody
        chart={chart}
        series={series}
        plotted={plotted}
        range={range}
        asTable={asTable}
        isPending={isPending}
        error={error}
      />
    </HistoryCard>
  );
}

function HistoryBody({
  chart,
  series,
  plotted,
  range,
  asTable,
  isPending,
  error,
}: {
  chart: ChartSpec;
  series: HistorySeries[];
  plotted: ReturnType<typeof plotSeries>;
  range: [number, number];
  asTable: boolean;
  isPending: boolean;
  error: Error | null;
}) {
  const t = useT();

  if (isPending)
    return <ScreenMessage loading>{t("history.loading")}</ScreenMessage>;

  if (error)
    return (
      <Alert variant="warning" title={t("history.failed")}>
        {error.message}
      </Alert>
    );

  if (series.length === 0)
    return <ScreenMessage>{t("history.empty")}</ScreenMessage>;

  if (asTable) return <HistoryTable series={series} />;

  return chart.kind === "strip" ? (
    <StateStrip series={series} range={range} plotted={plotted} />
  ) : (
    <LineChart
      unit={chart.unit}
      series={series}
      range={range}
      plotted={plotted}
    />
  );
}
