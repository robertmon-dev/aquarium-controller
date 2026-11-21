import { Badge } from "@/components/atoms";
import { useT } from "@aquarium/i18n/use-locale";
import { EM_DASH, PROBE_FRESHNESS_SECONDS } from "@aquarium/view-model";
import type { LiveProbe } from "./types";

export function LiveReading({ live, unit }: { live: LiveProbe; unit: string }) {
  const t = useT();

  const volts = live.volts === null ? EM_DASH : `${live.volts.toFixed(4)} V`;

  const note = !live.sampled
    ? t("calibration.neverSampled")
    : live.volts === null
      ? t("calibration.noProbe")
      : live.ageSeconds !== null && live.ageSeconds > PROBE_FRESHNESS_SECONDS
        ? t("calibration.staleReading", { seconds: String(live.ageSeconds) })
        : null;

  return (
    <div className="calibration__live">
      <span className="calibration__live-label">
        {t("calibration.liveVolts")}
      </span>
      <span className="calibration__live-value">{volts}</span>

      <span className="calibration__live-label">
        {t("calibration.liveReading")}
      </span>
      <span className="calibration__live-value">
        {`${live.reading} ${unit}`.trim()}
      </span>

      <Badge variant={live.stability === "stable" ? "success" : "secondary"}>
        {live.stability === "stable"
          ? t("calibration.stable")
          : t("calibration.settling")}
      </Badge>

      {note ? <Badge variant="warning">{note}</Badge> : null}
    </div>
  );
}
