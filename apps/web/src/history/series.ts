export type ChartKind = "line" | "step" | "strip";
export type Aggregate = "mean" | "last";

export interface ChartSpec {
  id: string;
  measurement: string;
  fields: string[];
  reference?: string;
  unit: string;
  kind: ChartKind;
  aggregate?: Aggregate;
  states?: readonly string[];
}

export function aggregateOf(chart: ChartSpec): Aggregate {
  return chart.aggregate ?? (chart.kind === "line" ? "mean" : "last");
}

export const CHARTS: ChartSpec[] = [
  {
    id: "temperature",
    measurement: "aquarium_temperature",
    fields: ["tank_c", "sump_c"],
    reference: "target_tank_c",
    unit: "°C",
    kind: "line",
  },
  {
    id: "ph",
    measurement: "aquarium_co2",
    fields: ["ph"],
    reference: "ph_target",
    unit: "pH",
    kind: "line",
  },
  {
    id: "electrode",
    measurement: "aquarium_co2",
    fields: ["electrode_health"],
    unit: "",
    kind: "line",
  },
  {
    id: "channels",
    measurement: "aquarium_lighting",
    fields: ["white_pct", "rgb_pct"],
    unit: "%",
    kind: "line",
  },
  {
    id: "flow",
    measurement: "aquarium_filtration",
    fields: ["flow_lph"],
    unit: "L/h",
    kind: "line",
  },
  {
    id: "rssi",
    measurement: "aquarium_health",
    fields: ["rssi_dbm"],
    unit: "dBm",
    kind: "line",
  },
  {
    id: "heap",
    measurement: "aquarium_health",
    fields: ["free_heap_kb"],
    unit: "KiB",
    kind: "line",
  },
  {
    id: "actuators",
    measurement: "aquarium_temperature",
    fields: ["heater_tank", "heater_sump"],
    unit: "",
    kind: "strip",
  },
  {
    id: "co2Valve",
    measurement: "aquarium_co2",
    fields: ["solenoid_open"],
    unit: "",
    kind: "strip",
  },
  {
    id: "filtrationState",
    measurement: "aquarium_filtration",
    fields: ["flow_ok", "relay"],
    unit: "",
    kind: "strip",
  },
  {
    id: "atoState",
    measurement: "aquarium_ato",
    fields: ["pump_active", "sump_low", "reservoir_empty"],
    unit: "",
    kind: "strip",
  },
  {
    id: "pumpsState",
    measurement: "aquarium_pumps",
    fields: ["pump_return", "uv", "pump_makeup", "pump_drain", "top_off"],
    unit: "",
    kind: "strip",
  },
  {
    id: "waterChangePhase",
    measurement: "aquarium_pumps",
    fields: ["water_change_phase"],
    unit: "",
    kind: "step",
    aggregate: "last",
    states: [
      "history.phases.idle",
      "history.phases.draining",
      "history.phases.settling",
      "history.phases.refilling",
      "history.phases.resuming",
      "history.phases.suspended",
    ],
  },
  {
    id: "systemState",
    measurement: "aquarium_health",
    fields: ["leak", "water_healthy"],
    unit: "",
    kind: "strip",
  },
  {
    id: "waterLevel",
    measurement: "aquarium_water_level",
    fields: ["wet", "faulty"],
    unit: "",
    kind: "strip",
  },
];
