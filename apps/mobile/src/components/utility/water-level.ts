import type {
  WaterLevelEntryDto,
  WaterLevelMetricsDto,
} from "@aquarium/api-client";
import type { ChipVariant, StatusTone } from "@aquarium/theme";

export interface WaterLevelChip {
  label: string;
  variant: ChipVariant;
}

export interface WaterLevelCardData {
  title: string;
  status: StatusTone;
  statusLabel: string;
  chips: WaterLevelChip[];
}

const SENSORS: [string, keyof WaterLevelMetricsDto][] = [
  ["main tank", "main_tank"],
  ["mechanical", "sump_mechanical"],
  ["biological", "sump_biological"],
  ["pump", "sump_pump"],
  ["reserve", "sump_reserve"],
];

function variantOf(entry: WaterLevelEntryDto): ChipVariant {
  if (entry.faulty) return "faulty";
  return entry.wet ? "wet" : "dry";
}

export function waterLevelCard(
  levels: WaterLevelMetricsDto,
): WaterLevelCardData {
  const chips = SENSORS.map(([label, key]) => ({
    label,
    variant: variantOf(levels[key] as WaterLevelEntryDto),
  }));

  const faulty = chips.filter((chip) => chip.variant === "faulty").length;

  if (faulty > 0) {
    return {
      title: "water level",
      status: "degraded",
      statusLabel: `${faulty} ${faulty === 1 ? "sensor" : "sensors"} faulty`,
      chips,
    };
  }

  return {
    title: "water level",
    status: levels.sensors_healthy ? "ok" : "off",
    statusLabel: levels.sensors_healthy ? "healthy" : "sensors off",
    chips,
  };
}
