export type SeriesKind = "number" | "boolean" | "string";

export interface HistoryPoint {
  time: number;
  value: number;
}

export interface HistorySeries {
  measurement: string;
  field: string;
  kind: SeriesKind;
  points: HistoryPoint[];
}

interface Block {
  datatypes: string[];
  header: string[];
  rows: string[][];
}

function splitBlocks(body: string): Block[] {
  const lines = body.replace(/\r/g, "").split("\n");
  const blocks: Block[] = [];
  let current: Block | null = null;

  for (const line of lines) {
    if (line.trim() === "") {
      if (current) blocks.push(current);
      current = null;
      continue;
    }

    const cells = line.split(",");

    if (line.startsWith("#datatype")) {
      current = { datatypes: cells.slice(1), header: [], rows: [] };
      continue;
    }

    if (line.startsWith("#")) continue;
    if (!current) continue;

    if (current.header.length === 0) {
      current.header = cells;
      continue;
    }

    current.rows.push(cells);
  }

  if (current) blocks.push(current);

  return blocks.filter((block) => block.header.length > 0);
}

function kindOf(datatype: string | undefined): SeriesKind {
  if (datatype === "boolean") return "boolean";
  if (
    datatype === "double" ||
    datatype === "long" ||
    datatype === "unsignedLong"
  )
    return "number";
  return "string";
}

function toValue(raw: string, kind: SeriesKind): number | null {
  if (raw === "") return null;
  if (kind === "boolean") return raw === "true" ? 1 : 0;

  const parsed = Number(raw);
  return Number.isFinite(parsed) ? parsed : null;
}

export function parseAnnotatedCsv(body: string): HistorySeries[] {
  const out = new Map<string, HistorySeries>();

  for (const block of splitBlocks(body)) {
    const index = (name: string) => block.header.indexOf(name);
    const timeAt = index("_time");
    const valueAt = index("_value");
    const fieldAt = index("_field");
    const measurementAt = index("_measurement");

    if (timeAt < 0 || valueAt < 0 || fieldAt < 0) continue;

    const kind = kindOf(block.datatypes[valueAt - 1]);

    for (const row of block.rows) {
      const field = row[fieldAt];
      const rawTime = row[timeAt];
      const rawValue = row[valueAt];
      if (!field || !rawTime || rawValue === undefined) continue;

      const value = toValue(rawValue, kind);
      if (value === null) continue;

      const time = Math.floor(Date.parse(rawTime) / 1000);
      if (!Number.isFinite(time)) continue;

      const measurement = measurementAt >= 0 ? (row[measurementAt] ?? "") : "";
      const key = `${measurement}:${field}`;

      const existing = out.get(key);
      if (existing) {
        existing.points.push({ time, value });
      } else {
        out.set(key, { measurement, field, kind, points: [{ time, value }] });
      }
    }
  }

  for (const series of out.values()) {
    series.points.sort((a, b) => a.time - b.time);
  }

  return [...out.values()];
}
