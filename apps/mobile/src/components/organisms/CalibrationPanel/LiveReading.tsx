import { View } from "react-native";
import { Badge, TextDisplay } from "@/components/atoms";
import { spacing } from "@aquarium/theme";
import { EM_DASH } from "@aquarium/view-model";
import { useT } from "@/i18n/use-locale";
import type { LiveProbe } from "./types";
import type { ReactNode } from "react";

export const LiveReading = ({
  live,
  unit,
}: {
  live: LiveProbe;
  unit: string;
}): ReactNode => {
  const t = useT();

  const volts = live.volts === null ? EM_DASH : `${live.volts.toFixed(4)} V`;

  const note = !live.sampled
    ? t("calibration.neverSampled")
    : live.volts === null
      ? t("calibration.noProbe")
      : live.ageSeconds !== null && live.ageSeconds > 10
        ? t("calibration.staleReading", { seconds: String(live.ageSeconds) })
        : undefined;

  return (
    <View style={{ gap: spacing.xs }}>
      <TextDisplay label={t("calibration.liveVolts")} fullWidth>
        {volts}
      </TextDisplay>

      <TextDisplay label={t("calibration.liveReading")} fullWidth>
        {`${live.reading} ${unit}`.trim()}
      </TextDisplay>

      <View style={{ flexDirection: "row", gap: spacing.xs }}>
        <Badge variant={live.stability === "stable" ? "success" : "secondary"}>
          {live.stability === "stable"
            ? t("calibration.stable")
            : t("calibration.settling")}
        </Badge>

        {note ? <Badge variant="warning">{note}</Badge> : null}
      </View>
    </View>
  );
};
