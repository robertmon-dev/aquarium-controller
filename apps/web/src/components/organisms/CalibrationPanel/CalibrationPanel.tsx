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
import { useT } from "@aquarium/i18n/use-locale";
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
import { useEffect, useMemo, useState } from "react";
import { useForm } from "react-hook-form";
import { Alert, Button, Card, Input } from "@/components/atoms";
import { calibrationFormMessages } from "@/device/calibration-messages";
import { PhSection } from "./PhSection";
import { TdsSection } from "./TdsSection";
import type { LiveProbe } from "./types";
import "./CalibrationPanel.scss";

const nowEpochSeconds = (): number => Math.floor(Date.now() / 1000);

export function CalibrationPanel({ transport }: { transport: Transport }) {
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

  const {
    register,
    handleSubmit,
    setValue,
    reset,
    formState: { errors },
  } = useForm<CalibrationFormValues, unknown, ParsedCalibrationForm>({
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
        <Button type="button" variant="secondary" onClick={() => setOpen(true)}>
          {t("calibration.expand")}
        </Button>
      </Card>
    );
  }

  return (
    <Card variant="outlined" padding="md" width="full">
      <form
        className="calibration"
        onSubmit={(event) => void onSave(event)}
        noValidate
      >
        <h3 className="calibration__title">{t("calibration.title")}</h3>

        <PhSection
          register={register}
          errors={errors}
          live={phLive}
          electrodeHealth={telemetry.data?.co2.ph_electrode_health ?? null}
          onCapture={onCapture}
        />

        <TdsSection
          register={register}
          errors={errors}
          live={tdsLive}
          reportedPpm={reportedPpm}
          onReferenceEntered={onReferenceEntered}
        />

        <section className="calibration__section">
          <h4 className="calibration__section-title">
            {t("calibration.makeupTitle")}
          </h4>

          <div className="calibration__row">
            <Input
              label={t("calibration.makeupTankCelsius")}
              inputMode="decimal"
              fullWidth
              helperText={t("calibration.makeupHelper")}
              {...register("makeupTankCelsius")}
              {...(errors.makeupTankCelsius?.message
                ? { error: errors.makeupTankCelsius.message }
                : {})}
            />
          </div>
        </section>

        {save.isError ? (
          <Alert variant="danger" title={t("calibration.saveFailed")}>
            {describeCalibrationFailure(save.error, t)}
          </Alert>
        ) : null}

        {save.isSuccess ? (
          <Alert variant="success">{t("calibration.saved")}</Alert>
        ) : null}

        <div className="calibration__footer">
          <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
            {t("calibration.collapse")}
          </Button>
          <Button type="submit" variant="primary" isLoading={save.isPending}>
            {t("calibration.save")}
          </Button>
        </div>
      </form>
    </Card>
  );
}
