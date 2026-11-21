import { View } from "react-native";
import { Button, Card, TextDisplay } from "@/components/atoms";
import { FormField } from "@/components/molecules";
import { spacing } from "@aquarium/theme";
import { canCapturePh, formatPercent } from "@aquarium/view-model";
import { useT } from "@/i18n/use-locale";
import { LiveReading } from "./LiveReading";
import type { PhSectionProps } from "./types";
import type { ReactNode } from "react";

export const PhSection = ({
  control,
  live,
  electrodeHealth,
  onCapture,
}: PhSectionProps): ReactNode => {
  const t = useT();
  const canCapture = canCapturePh(live);

  return (
    <Card variant="outlined" padding="sm" width="full">
      <View style={{ gap: spacing.sm }}>
        <TextDisplay label={t("calibration.phTitle")} fullWidth>
          {electrodeHealth === null
            ? t("calibration.electrodeHealth")
            : `${t("calibration.electrodeHealth")}: ${formatPercent(
                Math.round(electrodeHealth * 100),
              )}`}
        </TextDisplay>

        <LiveReading live={live} unit="pH" />

        <View style={{ flexDirection: "row", gap: spacing.xs }}>
          <Button
            variant="secondary"
            disabled={!canCapture}
            onPress={() => onCapture("neutralVolts")}
            style={{ flex: 1 }}
          >
            {t("calibration.captureNeutral")}
          </Button>
          <Button
            variant="secondary"
            disabled={!canCapture}
            onPress={() => onCapture("acidVolts")}
            style={{ flex: 1 }}
          >
            {t("calibration.captureAcid")}
          </Button>
        </View>

        <FormField
          control={control}
          name="neutralVolts"
          label={t("calibration.neutralVolts")}
          keyboardType="decimal-pad"
        />
        <FormField
          control={control}
          name="acidVolts"
          label={t("calibration.acidVolts")}
          keyboardType="decimal-pad"
        />
        <FormField
          control={control}
          name="calibratedAtCelsius"
          label={t("calibration.calibratedAtCelsius")}
          keyboardType="decimal-pad"
          helperText={t("calibration.calibratedAtHelper")}
        />
      </View>
    </Card>
  );
};
