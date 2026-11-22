import { useState } from "react";
import { Button, Input } from "@/components/atoms";
import { useT } from "@aquarium/i18n/use-locale";
import { canUseTdsReading } from "@aquarium/view-model";
import { LiveReading } from "./LiveReading";
import type { TdsSectionProps } from "./types";

export function TdsSection({
  register,
  errors,
  live,
  reportedPpm,
  onReferenceEntered,
}: TdsSectionProps) {
  const t = useT();
  const [reference, setReference] = useState("");
  const hasReading = canUseTdsReading({
    reportedPpm,
    ageSeconds: live.ageSeconds,
  });

  return (
    <section className="calibration__section">
      <h4 className="calibration__section-title">
        {t("calibration.tdsTitle")}
      </h4>

      <LiveReading live={live} unit="ppm" />

      <div className="calibration__row">
        <Input
          label={t("calibration.tdsFactor")}
          inputMode="decimal"
          fullWidth
          {...register("tdsFactor")}
          {...(errors.tdsFactor?.message
            ? { error: errors.tdsFactor.message }
            : {})}
        />
        <Input
          label={t("calibration.referencePpm")}
          inputMode="decimal"
          fullWidth
          value={reference}
          onChange={(event) => setReference(event.target.value)}
          helperText={
            hasReading
              ? t("calibration.referenceHelper")
              : t("calibration.referenceNoReading")
          }
        />
      </div>

      <div className="calibration__actions">
        <Button
          type="button"
          variant="secondary"
          disabled={!hasReading || reference.trim() === ""}
          onClick={() => onReferenceEntered(reference)}
        >
          {t("calibration.referenceHelper")}
        </Button>
      </div>
    </section>
  );
}
