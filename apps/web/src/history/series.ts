export type ChartKind = "line" | "strip";

export interface ChartSpec {
  id: string;
  measurement: string;
  fields: string[];
  reference?: string;
  unit: string;
  kind: ChartKind;
}

export const CHARTS: ChartSpec[] = [
  {
    id: "temperature",
    measurement: "aquarium_temperature",
    fields: ["tank_c", "sump_c"],
    reference: "target_c",
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
    fields: ["heater"],
    unit: "",
    kind: "strip",
  },
  {
    id: "co2-valve",
    measurement: "aquarium_co2",
    fields: ["solenoid_open"],
    unit: "",
    kind: "strip",
  },
  {
    id: "filtration-state",
    measurement: "aquarium_filtration",
    fields: ["flow_ok", "relay"],
    unit: "",
    kind: "strip",
  },
  {
    id: "ato-state",
    measurement: "aquarium_ato",
    fields: ["pump_active", "sump_low", "reservoir_empty"],
    unit: "",
    kind: "strip",
  },
  {
    id: "system-state",
    measurement: "aquarium_health",
    fields: ["leak", "water_healthy"],
    unit: "",
    kind: "strip",
  },
  {
    id: "water-level",
    measurement: "aquarium_water_level",
    fields: ["wet", "faulty"],
    unit: "",
    kind: "strip",
  },
];
