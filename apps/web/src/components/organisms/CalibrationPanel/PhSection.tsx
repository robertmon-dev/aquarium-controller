import { Button, Input } from "@/components/atoms";
import { useT } from "@aquarium/i18n/use-locale";
import { canCapturePh, formatPercent } from "@aquarium/view-model";
import { LiveReading } from "./LiveReading";
import type { PhSectionProps } from "./types";

export function PhSection({
  register,
  errors,
  live,
  electrodeHealth,
  onCapture,
}: PhSectionProps) {
  const t = useT();
  const canCapture = canCapturePh(live);

  return (
    <section className="calibration__section">
      <h4 className="calibration__section-title">
        {t("calibration.phTitle")}
        {electrodeHealth === null
          ? null
          : ` — ${t("calibration.electrodeHealth")}: ${formatPercent(
              Math.round(electrodeHealth * 100),
            )}`}
      </h4>

      <LiveReading live={live} unit="pH" />

      <div className="calibration__actions">
        <Button
          type="button"
          variant="secondary"
          disabled={!canCapture}
          onClick={() => onCapture("neutralVolts")}
        >
          {t("calibration.captureNeutral")}
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={!canCapture}
          onClick={() => onCapture("acidVolts")}
        >
          {t("calibration.captureAcid")}
        </Button>
      </div>

      <div className="calibration__row">
        <Input
          label={t("calibration.neutralVolts")}
          inputMode="decimal"
          fullWidth
          {...register("neutralVolts")}
          {...(errors.neutralVolts?.message
            ? { error: errors.neutralVolts.message }
            : {})}
        />
        <Input
          label={t("calibration.acidVolts")}
          inputMode="decimal"
          fullWidth
          {...register("acidVolts")}
          {...(errors.acidVolts?.message
            ? { error: errors.acidVolts.message }
            : {})}
        />
        <Input
          label={t("calibration.calibratedAtCelsius")}
          inputMode="decimal"
          fullWidth
          helperText={t("calibration.calibratedAtHelper")}
          {...register("calibratedAtCelsius")}
          {...(errors.calibratedAtCelsius?.message
            ? { error: errors.calibratedAtCelsius.message }
            : {})}
        />
      </div>
    </section>
  );
}
