import type { HistorySeries } from "@/history/csv";
import { formatSample } from "./readings";
import { historyTableModel } from "./table-rows";
import "./HistoryPanel.scss";

export function HistoryTable({ series }: { series: HistorySeries[] }) {
  const { columns, rows } = historyTableModel(series);

  return (
    <div className="history-table">
      <table>
        <thead>
          <tr>
            <th scope="col">time</th>
            {columns.map((column) => (
              <th scope="col" key={column.field}>
                {column.field}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.time}>
              <th scope="row">
                {new Date(row.time * 1000).toLocaleTimeString()}
              </th>
              {row.cells.map((value, index) => {
                const column = columns[index];
                if (!column) return null;

                return (
                  <td key={column.field}>{formatSample(value, column.kind)}</td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
