import {
  calibrationFormSchema,
  toCalibrationFormValues,
  toCalibrationPatch,
  useCalibration,
  usePatchCalibration,
  useProbeVoltages,
  useTelemetry,
  type CalibrationFormValues,
  type ParsedCalibrationForm,
  type Transport,
} from "@aquarium/api-client";
import {
  canCapturePh,
  describeCalibrationFailure,
  EM_DASH,
  formatPh,
  probeAge,
  probeStability,
  tdsFactorFromReference,
  useProbeSamples,
} from "@aquarium/view-model";
import { zodResolver } from "@hookform/resolvers/zod";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useForm } from "react-hook-form";
import { View } from "react-native";
import { Alert, Button, Card, TextDisplay } from "@/components/atoms";
import { FormField } from "@/components/molecules";
import { spacing } from "@aquarium/theme";
import { calibrationFormMessages } from "@/device/calibration-messages";
import { useT } from "@/i18n/use-locale";
import { PhSection } from "./PhSection";
import { TdsSection } from "./TdsSection";
import type { LiveProbe } from "./types";

const nowEpochSeconds = (): number => Math.floor(Date.now() / 1000);

export const CalibrationPanel = ({
  transport,
}: {
  transport: Transport;
}): ReactNode => {
  const t = useT();
  const [open, setOpen] = useState(false);

  const calibration = useCalibration(transport);
  const probes = useProbeVoltages(transport, { enabled: open });
  const telemetry = useTelemetry(transport);
  const samples = useProbeSamples(probes.data);
  const save = usePatchCalibration(transport);

  const schema = useMemo(
    () => calibrationFormSchema(calibrationFormMessages(t)),
    [t],
  );

  const { control, handleSubmit, setValue, reset } = useForm<
    CalibrationFormValues,
    unknown,
    ParsedCalibrationForm
  >({
    resolver: zodResolver(schema),
    defaultValues: {
      neutralVolts: "",
      acidVolts: "",
      calibratedAtCelsius: "",
      makeupTankCelsius: "",
      tdsFactor: "",
    },
  });

  useEffect(() => {
    if (calibration.data) reset(toCalibrationFormValues(calibration.data));
  }, [calibration.data, reset]);

  const sampled = (probes.data?.sampled_at_epoch_s ?? 0) > 0;
  const age = probes.data
    ? probeAge(probes.data.sampled_at_epoch_s, nowEpochSeconds())
    : null;

  const phLive: LiveProbe = {
    volts: probes.data?.ph ?? null,
    reading: formatPh(telemetry.data?.co2.current_ph ?? null),
    stability: probeStability(samples.ph),
    ageSeconds: age,
    sampled,
  };

  const reportedPpm = telemetry.data?.tds.ppm ?? null;

  const tdsLive: LiveProbe = {
    volts: probes.data?.tds ?? null,
    reading: reportedPpm === null ? EM_DASH : String(Math.round(reportedPpm)),
    stability: probeStability(samples.tds),
    ageSeconds: age,
    sampled,
  };

  const onCapture = (field: "neutralVolts" | "acidVolts") => {
    const reading = probes.data;
    if (!reading || reading.ph === null) return;

    const fresh = canCapturePh({
      stability: probeStability(samples.ph),
      volts: reading.ph,
      ageSeconds: probeAge(reading.sampled_at_epoch_s, nowEpochSeconds()),
    });
    if (!fresh) return;

    setValue(field, reading.ph.toFixed(4), { shouldValidate: true });
  };

  const onReferenceEntered = (referencePpm: string) => {
    const reference = Number(referencePpm.replace(",", "."));
    const currentFactor = calibration.data?.tds_factor;

    if (reportedPpm === null || currentFactor === undefined) return;

    const factor = tdsFactorFromReference({
      reportedPpm,
      currentFactor,
      referencePpm: reference,
    });

    if (factor === null) return;

    setValue("tdsFactor", factor.toFixed(3), { shouldValidate: true });
  };

  const onSave = handleSubmit(async (values) => {
    if (!calibration.data) return;

    await save.mutateAsync(toCalibrationPatch(values, calibration.data));
  });

  if (!open) {
    return (
      <Card variant="outlined" padding="md" width="full">
        <Button variant="secondary" fullWidth onPress={() => setOpen(true)}>
          {t("calibration.expand")}
        </Button>
      </Card>
    );
  }

  return (
    <Card variant="outlined" padding="md" width="full">
      <View style={{ gap: spacing.sm }}>
        <PhSection
          control={control}
          live={phLive}
          electrodeHealth={telemetry.data?.co2.ph_electrode_health ?? null}
          onCapture={onCapture}
        />

        <TdsSection
          control={control}
          live={tdsLive}
          reportedPpm={reportedPpm}
          onReferenceEntered={onReferenceEntered}
        />

        <Card variant="outlined" padding="sm" width="full">
          <View style={{ gap: spacing.xs }}>
            <TextDisplay label={t("calibration.makeupTitle")} fullWidth>
              {t("calibration.makeupHelper")}
            </TextDisplay>

            <FormField
              control={control}
              name="makeupTankCelsius"
              label={t("calibration.makeupTankCelsius")}
              keyboardType="decimal-pad"
            />
          </View>
        </Card>

        {save.isError ? (
          <Alert variant="danger" title={t("calibration.saveFailed")}>
            {describeCalibrationFailure(save.error, t)}
          </Alert>
        ) : null}

        {save.isSuccess ? (
          <Alert variant="success">{t("calibration.saved")}</Alert>
        ) : null}

        <Button
          variant="primary"
          fullWidth
          isLoading={save.isPending}
          onPress={() => void onSave()}
        >
          {t("calibration.save")}
        </Button>

        <Button variant="ghost" fullWidth onPress={() => setOpen(false)}>
          {t("calibration.collapse")}
        </Button>
      </View>
    </Card>
  );
};
