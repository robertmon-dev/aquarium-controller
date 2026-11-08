import type { HistorySeries } from "@/history/csv";
import "./HistoryPanel.scss";

export function HistoryTable({ series }: { series: HistorySeries[] }) {
  const times = [
    ...new Set(series.flatMap((one) => one.points.map((p) => p.time))),
  ]
    .sort((a, b) => b - a)
    .slice(0, 200);

  const byField = series.map((one) => ({
    field: one.field,
    kind: one.kind,
    lookup: new Map(one.points.map((p) => [p.time, p.value])),
  }));

  return (
    <div className="history-table">
      <table>
        <thead>
          <tr>
            <th scope="col">time</th>
            {byField.map((one) => (
              <th scope="col" key={one.field}>
                {one.field}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {times.map((time) => (
            <tr key={time}>
              <th scope="row">{new Date(time * 1000).toLocaleTimeString()}</th>
              {byField.map((one) => {
                const value = one.lookup.get(time);

                return (
                  <td key={one.field}>
                    {value === undefined
                      ? "—"
                      : one.kind === "boolean"
                        ? value === 1
                          ? "on"
                          : "off"
                        : value.toFixed(2)}
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
