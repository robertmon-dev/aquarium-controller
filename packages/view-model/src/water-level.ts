import type {
  WaterLevelEntryDto,
  WaterLevelMetricsDto,
} from "@aquarium/api-client";
import type { Translate, TranslationKey } from "@aquarium/i18n";
import type { ChipVariant, StatusTone } from "./semantics";

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

const SENSORS: [TranslationKey, keyof WaterLevelMetricsDto][] = [
  ["water.mainTank", "main_tank"],
  ["water.mechanical", "sump_mechanical"],
  ["water.biological", "sump_biological"],
  ["water.pump", "sump_pump"],
  ["water.reserve", "sump_reserve"],
];

function variantOf(entry: WaterLevelEntryDto): ChipVariant {
  if (entry.faulty) return "faulty";
  return entry.wet ? "wet" : "dry";
}

export function waterLevelCard(
  levels: WaterLevelMetricsDto,
  t: Translate,
): WaterLevelCardData {
  const chips = SENSORS.map(([key, field]) => ({
    label: t(key),
    variant: variantOf(levels[field] as WaterLevelEntryDto),
  }));

  const faulty = chips.filter((chip) => chip.variant === "faulty").length;

  if (faulty > 0) {
    return {
      title: t("water.title"),
      status: "degraded",
      statusLabel:
        faulty === 1
          ? t("water.faultyOne")
          : t("water.faultyMany", { count: faulty }),
      chips,
    };
  }

  return {
    title: t("water.title"),
    status: levels.sensors_healthy ? "ok" : "off",
    statusLabel: levels.sensors_healthy
      ? t("common.healthy")
      : t("common.sensorsOff"),
    chips,
  };
}
