import { useState } from "react";
import { View } from "react-native";
import { Button, Card, Input, TextDisplay } from "@/components/atoms";
import { FormField } from "@/components/molecules";
import { spacing } from "@aquarium/theme";
import { useT } from "@/i18n/use-locale";
import { canUseTdsReading } from "@aquarium/view-model";
import { LiveReading } from "./LiveReading";
import type { TdsSectionProps } from "./types";
import type { ReactNode } from "react";

export const TdsSection = ({
  control,
  live,
  reportedPpm,
  onReferenceEntered,
}: TdsSectionProps): ReactNode => {
  const t = useT();
  const [reference, setReference] = useState("");
  const hasReading = canUseTdsReading({
    reportedPpm,
    ageSeconds: live.ageSeconds,
  });

  return (
    <Card variant="outlined" padding="sm" width="full">
      <View style={{ gap: spacing.sm }}>
        <TextDisplay label={t("calibration.tdsTitle")} fullWidth>
          {t("calibration.tdsFactor")}
        </TextDisplay>

        <LiveReading live={live} unit="ppm" />

        <FormField
          control={control}
          name="tdsFactor"
          label={t("calibration.tdsFactor")}
          keyboardType="decimal-pad"
        />

        <Input
          label={t("calibration.referencePpm")}
          value={reference}
          onChangeText={setReference}
          keyboardType="decimal-pad"
          fullWidth
          helperText={
            hasReading
              ? t("calibration.referenceHelper")
              : t("calibration.referenceNoReading")
          }
        />

        <Button
          variant="secondary"
          disabled={!hasReading || reference.trim() === ""}
          onPress={() => onReferenceEntered(reference)}
        >
          {t("calibration.referenceHelper")}
        </Button>
      </View>
    </Card>
  );
};
